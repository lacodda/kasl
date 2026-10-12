//! Inbox rules: read one at a time, matched with the list's own cuts, acted on
//! when an issue comes to match them - and never again until it leaves and
//! comes back.
//!
//! Each property has a test that would fail without it: a rule re-applied on
//! every poll, a typo read as "match everything", a dismiss rule hiding work
//! in hand, a pin beating a dismissal.

use chrono::{Duration, Local};
use kasl::api::Session;
use kasl::api::jira::{Jira, JiraConfig};
use kasl::db::jira_inbox::{Include, JiraInbox, JiraInboxItem, JiraInboxUpsert};
use kasl::db::tags::Tags;
use kasl::db::tasks::Tasks;
use kasl::libs::config::{Config, JiraInboxConfig};
use kasl::libs::data_storage::DataStorage;
use kasl::libs::inbox_rules::{self, InboxRule, RuleBook, Verdict};
use kasl::libs::jira_bridge::MoveOptions;
use kasl::libs::jira_inbox::{Notice, TOAST_STORM_THRESHOLD, ToastBudget, ToastPlan, notices, sync_noninteractive};
use kasl::libs::task::TaskFilter;
use kasl::libs::toast_apply::{self, Applied};
use serde_json::{Value, json};
use serial_test::serial;
use std::path::Path;
use std::process::{Command, Stdio};
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A throwaway data directory, so nothing reads or writes the machine's own.
fn sandbox() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    // SAFETY: tests touching the env are #[serial]
    unsafe {
        std::env::set_var("HOME", dir.path());
        std::env::set_var("LOCALAPPDATA", dir.path());
    }
    dir
}

/// The inbox config with these rules, toasts off, pointed at `api_url`.
fn configure(api_url: &str, rules: Value) -> JiraInboxConfig {
    let inbox = JiraInboxConfig {
        notify: false,
        notify_changes: false,
        notify_gone: false,
        rules: rules.as_array().cloned().unwrap_or_default(),
        ..Default::default()
    };
    Config {
        jira: Some(JiraConfig {
            login: "agent".to_string(),
            api_url: api_url.to_string(),
            completed_statuses: Vec::new(),
        }),
        jira_inbox: Some(inbox.clone()),
        ..Default::default()
    }
    .save()
    .unwrap();
    let session = DataStorage::new().get_path(".jira_session_id").unwrap();
    Jira::write_session_id(session.to_str().unwrap(), "JSESSIONID=cafe").unwrap();
    inbox
}

/// An issue as the Jira search returns it.
fn issue(key: &str, priority: (&str, i32), issue_type: &str) -> Value {
    let project = key.split('-').next().unwrap();
    json!({
        "id": "1",
        "key": key,
        "fields": {
            "summary": format!("Summary for {key}"),
            "status": {"id": "10", "name": "Open"},
            "priority": {"name": priority.0, "id": priority.1.to_string()},
            "issuetype": {"name": issue_type},
            "project": {"key": project},
            "updated": "2026-10-12T10:00:00.000+0000"
        }
    })
}

/// Makes Jira answer the next poll with exactly these issues.
async fn jira_holds(server: &MockServer, issues: Vec<Value>) {
    server.reset().await;
    Mock::given(method("GET"))
        .and(path("/rest/api/2/search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "startAt": 0, "maxResults": 100, "total": issues.len(),
            "issues": issues
        })))
        .mount(server)
        .await;
}

fn row(key: &str) -> JiraInboxItem {
    JiraInbox::new().unwrap().get_by_key(key).unwrap().unwrap()
}

fn upsert(key: &str, priority: (&str, i32), issue_type: &str) -> JiraInboxUpsert {
    JiraInboxUpsert {
        issue_key: key.to_string(),
        issue_id: "1".to_string(),
        summary: format!("Summary for {key}"),
        status_id: Some("10".to_string()),
        status_name: "Open".to_string(),
        priority: Some(priority.0.to_string()),
        priority_rank: priority.1,
        sort_value: None,
        url: format!("https://jira.example.com/browse/{key}"),
        raw_updated: None,
        issue_type: Some(issue_type.to_string()),
        project: key.split('-').next().map(str::to_string),
    }
}

/// A row in memory, for rules that need no database.
fn item(key: &str, priority: (&str, i32), issue_type: Option<&str>) -> JiraInboxItem {
    let now = Local::now().naive_local();
    JiraInboxItem {
        issue_key: key.to_string(),
        issue_id: "1".to_string(),
        summary: String::new(),
        status_id: Some("10".to_string()),
        status_name: "Open".to_string(),
        priority: Some(priority.0.to_string()),
        priority_rank: priority.1,
        sort_value: Some(5.0),
        url: String::new(),
        first_seen: now,
        last_seen: now,
        notified: true,
        pinned: false,
        dismissed: false,
        raw_updated: None,
        gone_at: None,
        last_change: None,
        changed_at: None,
        taken_at: None,
        snoozed_until: None,
        woke_at: None,
        left_reason: None,
        resolution: None,
        issue_type: issue_type.map(str::to_string),
        project: key.split('-').next().map(str::to_string),
        ruled_by: None,
    }
}

const PRIORITIES: [(&str, i32); 4] = [("Highest", 1), ("High", 2), ("Medium", 3), ("Low", 4)];

fn priorities() -> Vec<(String, i32)> {
    PRIORITIES.iter().map(|(name, rank)| (name.to_string(), *rank)).collect()
}

fn book(rules: Value) -> RuleBook {
    RuleBook::compile(rules.as_array().unwrap(), &priorities())
}

// --- Reading rules ---

#[test]
fn a_rule_reads_as_written() {
    let rule = InboxRule::parse(&json!({
        "name": "on fire",
        "when": {"priority": "High+", "min_score": 8, "type": ["Bug", "Incident"], "project": "PAY", "status": "Open"},
        "then": {"pin": true, "urgent": true, "tag": ["payments", "fire"]}
    }))
    .unwrap();
    assert_eq!(rule.when.types, vec!["Bug", "Incident"], "a list is any of its names");
    assert_eq!(rule.when.projects, vec!["PAY"], "one name is a list of one");
    assert_eq!(rule.then.tag, vec!["payments", "fire"]);
    assert_eq!(rule.label(), "on fire");

    let unnamed = InboxRule::parse(&json!({"when": {"priority": "High+", "type": ["Epic", "Story"]}, "then": {"pin": true}})).unwrap();
    assert_eq!(
        unnamed.label(),
        "priority High+, type Epic or Story",
        "without a name a rule is called by its conditions"
    );
}

#[test]
fn a_misspelt_key_is_refused_not_ignored() {
    // Ignored, `prority` would leave an empty `when` - every issue - and the
    // dismiss would empty the inbox. The refusal must name the word.
    let err = InboxRule::parse(&json!({"when": {"prority": "High", "type": "Epic"}, "then": {"dismiss": true}})).unwrap_err();
    assert!(err.contains("prority"), "the refusal names the misspelt key: {err}");

    let err = InboxRule::parse(&json!({"when": {"type": "Epic"}, "then": {"dismis": true}})).unwrap_err();
    assert!(err.contains("dismis"), "{err}");

    let err = InboxRule::parse(&json!({"when": {"type": "Epic"}, "then": {"dismiss": true}, "enabled": false})).unwrap_err();
    assert!(err.contains("enabled"), "{err}");
}

#[test]
fn a_rule_that_cannot_act_as_written_is_refused() {
    let refused = [
        (json!({"when": {"type": "Epic"}, "then": {}}), "does nothing"),
        (json!({"when": {"type": "Epic"}, "then": {"dismiss": true, "pin": true}}), "both dismisses"),
        (json!({"when": {"type": "Epic"}, "then": {"dismiss": true, "urgent": true}}), "both dismisses"),
        (json!({"when": {}, "then": {"dismiss": true}}), "every issue"),
        (json!({"then": {"dismiss": true}}), "every issue"),
        (json!({"when": {"type": "Bug"}, "then": {"tag": ["ok", " "]}}), "tag name"),
        (json!({"when": {"priority": "+"}, "then": {"pin": true}}), "priority"),
        (json!({"when": {"type": 7}, "then": {"pin": true}}), "expected a name"),
        (json!({"when": {"type": "Bug"}}), "then"),
    ];
    for (rule, why) in refused {
        let err = InboxRule::parse(&rule).expect_err(&format!("accepted: {rule}"));
        assert!(err.contains(why), "{rule}: expected '{why}' in: {err}");
    }

    // An empty `when` is fine for what cannot hide anything.
    assert!(InboxRule::parse(&json!({"then": {"tag": "jira"}})).is_ok());
}

#[test]
fn a_bad_rule_is_skipped_and_the_rest_still_act() {
    let book = book(json!([
        {"when": {"prority": "High"}, "then": {"dismiss": true}},
        {"when": {"priority": "High+"}, "then": {"pin": true}}
    ]));
    assert_eq!(book.problems.len(), 1);
    assert_eq!(book.problems[0].number, 1, "the problem names the rule's place in the config");
    assert_eq!(book.rules.len(), 1);
    assert_eq!(book.rules[0].number, 2);
    assert!(book.verdict(&item("KA-1", ("High", 2), None)).pin.is_some());
}

#[test]
#[serial]
fn a_bad_rule_does_not_make_the_config_unreadable() {
    let _dir = sandbox();
    let path = DataStorage::new().get_path("config.json").unwrap();
    std::fs::write(
        &path,
        json!({"jira_inbox": {"rules": [{"when": {"prority": "High"}, "then": {"dismiss": true}}, "not even a rule"]}}).to_string(),
    )
    .unwrap();

    // The watcher reads this file for activity tracking too.
    let config = Config::read().expect("a bad rule must not make the whole config unreadable");
    let inbox = config.jira_inbox.unwrap();
    let book = RuleBook::compile(&inbox.rules, &[]);
    assert_eq!(book.problems.len(), 2);
    assert!(book.rules.is_empty());

    // And saving the config keeps the rules as the person wrote them.
    Config::read().unwrap().save().unwrap();
    let again = Config::read().unwrap().jira_inbox.unwrap();
    assert_eq!(again.rules, inbox.rules, "a rewrite of the config must not drop or alter the rules");
}

// --- Matching ---

#[test]
fn priority_and_above_is_read_against_the_priorities_seen() {
    let and_above = book(json!([{"when": {"priority": "High+"}, "then": {"pin": true}}]));
    assert!(and_above.rules[0].matches(&item("KA-1", ("Highest", 1), None)));
    assert!(and_above.rules[0].matches(&item("KA-2", ("High", 2), None)));
    assert!(!and_above.rules[0].matches(&item("KA-3", ("Medium", 3), None)));

    let exact = book(json!([{"when": {"priority": "high"}, "then": {"pin": true}}]));
    assert!(exact.rules[0].matches(&item("KA-2", ("High", 2), None)), "names match whatever the case");
    assert!(!exact.rules[0].matches(&item("KA-1", ("Highest", 1), None)), "without + only that priority");
}

#[test]
fn a_priority_the_inbox_never_saw_matches_nothing_and_says_so() {
    let book = book(json!([{"when": {"priority": "Hgh+"}, "then": {"pin": true}}]));
    let rule = &book.rules[0];
    let why = rule.dormant.as_deref().expect("an unknown priority leaves the rule dormant");
    assert!(why.contains("Hgh") && why.contains("High"), "it names the word and what is known: {why}");
    assert!(!rule.matches(&item("KA-1", ("Highest", 1), None)));
}

#[test]
fn type_and_project_match_any_of_their_names() {
    let book = book(json!([{"when": {"type": ["Epic", "Story"], "project": "pay"}, "then": {"pin": true}}]));
    let rule = &book.rules[0];
    assert!(rule.matches(&item("PAY-1", ("Low", 4), Some("epic"))));
    assert!(rule.matches(&item("PAY-2", ("Low", 4), Some("Story"))));
    assert!(!rule.matches(&item("PAY-3", ("Low", 4), Some("Bug"))), "a type not named");
    assert!(!rule.matches(&item("OPS-1", ("Low", 4), Some("Epic"))), "a project not named");
    assert!(!rule.matches(&item("PAY-4", ("Low", 4), None)), "an issue whose type is not known yet");
}

#[test]
fn a_rule_counts_only_when_an_issue_comes_to_match_it() {
    let book = book(json!([{"name": "hot", "when": {"priority": "High+"}, "then": {"pin": true, "urgent": true}}]));
    let medium = item("KA-1", ("Medium", 3), None);
    let high = item("KA-1", ("High", 2), None);

    let arrived = book.entering(None, &high);
    assert_eq!(arrived.pin.as_deref(), Some("hot"), "an issue arriving matching enters the rule");
    assert_eq!(arrived.urgent.as_deref(), Some("hot"));
    assert_eq!(book.entering(Some(&medium), &high).pin.as_deref(), Some("hot"), "an escalation enters the rule");
    assert_eq!(
        book.entering(Some(&high), &high),
        Verdict::default(),
        "an issue already in the rule does not enter it again"
    );
    assert_eq!(book.entering(Some(&high), &medium), Verdict::default(), "leaving a rule does nothing");
}

#[test]
fn every_matching_rule_has_its_say() {
    let book = book(json!([
        {"name": "hot", "when": {"priority": "High+"}, "then": {"pin": true}},
        {"name": "epics", "when": {"type": "Epic"}, "then": {"dismiss": true}},
        {"when": {"project": "PAY"}, "then": {"tag": ["payments", "money"]}},
        {"when": {"type": "Epic"}, "then": {"tag": "money"}}
    ]));
    let verdict = book.verdict(&item("PAY-1", ("High", 2), Some("Epic")));
    assert_eq!(verdict.pin.as_deref(), Some("hot"));
    assert_eq!(verdict.dismiss.as_deref(), Some("epics"));
    assert_eq!(verdict.tags, vec!["payments", "money"], "tags gather once each, in rule order");
}

// --- One toast per issue ---

#[test]
fn an_urgent_issue_gets_one_toast_and_it_is_the_urgent_one() {
    let fresh = vec!["KA-1".to_string(), "KA-2".to_string()];
    let changed = vec![
        ("KA-3".to_string(), "↑prio High".to_string()),
        ("KA-4".to_string(), "status→Review".to_string()),
    ];
    let urgent = vec![
        ("KA-1".to_string(), "hot".to_string()),
        ("KA-3".to_string(), "hot".to_string()),
        ("KA-5".to_string(), "hot".to_string()),
    ];
    let got = notices(&fresh, &changed, &urgent);
    assert_eq!(
        got,
        vec![
            ("KA-1".to_string(), Notice::Urgent("hot".to_string())),
            ("KA-2".to_string(), Notice::New),
            ("KA-4".to_string(), Notice::Changed("status→Review".to_string())),
            ("KA-3".to_string(), Notice::Urgent("hot".to_string())),
            ("KA-5".to_string(), Notice::Urgent("hot".to_string())),
        ],
        "new and changed toasts give way to the urgent one, and a score-made urgency still toasts"
    );
}

#[test]
fn an_afternoon_of_ordinary_toasts_leaves_room_for_an_urgent_one() {
    let now = Local::now().naive_local();
    let mut budget = ToastBudget::new();
    assert_eq!(budget.admit(now, TOAST_STORM_THRESHOLD), ToastPlan::Each);
    assert_eq!(budget.admit(now, 1), ToastPlan::Summary, "the ordinary lane is spent");
    assert_eq!(budget.admit_urgent(now, 2), ToastPlan::Each, "urgent toasts count in a lane of their own");
    assert_eq!(
        budget.admit_urgent(now, TOAST_STORM_THRESHOLD),
        ToastPlan::Summary,
        "an urgent storm still collapses"
    );
}

// --- Against a poll ---

#[tokio::test]
#[serial]
async fn a_rule_pins_an_arrival_and_an_unpin_by_hand_stands() {
    let _dir = sandbox();
    let server = MockServer::start().await;
    let inbox = configure(&server.uri(), json!([{"name": "hot", "when": {"priority": "High+"}, "then": {"pin": true}}]));

    jira_holds(&server, vec![issue("KA-1", ("High", 2), "Bug"), issue("KA-2", ("Medium", 3), "Bug")]).await;
    let outcome = sync_noninteractive(&inbox, &mut ToastBudget::new()).await.unwrap();
    assert!(row("KA-1").pinned, "an issue arriving High is pinned");
    assert_eq!(row("KA-1").ruled_by.as_deref(), Some("hot"), "and the row names the rule");
    assert!(!row("KA-2").pinned);
    assert_eq!(outcome.ruled.len(), 1);
    assert_eq!(row("KA-1").issue_type.as_deref(), Some("Bug"), "the poll stores the type");
    assert_eq!(row("KA-1").project.as_deref(), Some("KA"), "and the project");

    JiraInbox::new().unwrap().set_pinned("KA-1", false).unwrap();
    assert_eq!(row("KA-1").ruled_by, None, "a hand takes the decision back");
    sync_noninteractive(&inbox, &mut ToastBudget::new()).await.unwrap();
    assert!(!row("KA-1").pinned, "the person's unpin stands while nothing changes");

    jira_holds(&server, vec![issue("KA-1", ("Medium", 3), "Bug"), issue("KA-2", ("Highest", 1), "Bug")]).await;
    sync_noninteractive(&inbox, &mut ToastBudget::new()).await.unwrap();
    assert!(row("KA-2").pinned, "an escalation is pinned");
    assert!(!row("KA-1").pinned);

    jira_holds(&server, vec![issue("KA-1", ("High", 2), "Bug"), issue("KA-2", ("Highest", 1), "Bug")]).await;
    sync_noninteractive(&inbox, &mut ToastBudget::new()).await.unwrap();
    assert!(row("KA-1").pinned, "leaving the rule and coming back is news, and pins again");
}

#[tokio::test]
#[serial]
async fn a_dismiss_rule_hides_an_arrival_but_never_work_in_hand() {
    let _dir = sandbox();
    let server = MockServer::start().await;
    let inbox = configure(
        &server.uri(),
        json!([{"name": "no epics", "when": {"type": "Epic"}, "then": {"dismiss": true}}]),
    );

    // KA-2 is already taken when Jira turns it into an Epic.
    let db = JiraInbox::new().unwrap();
    db.upsert_batch(&[upsert("KA-2", ("Medium", 3), "Story")]).unwrap();
    db.set_taken("KA-2", true).unwrap();

    jira_holds(&server, vec![issue("KA-1", ("Medium", 3), "Epic"), issue("KA-2", ("Medium", 3), "Epic")]).await;
    let outcome = sync_noninteractive(&inbox, &mut ToastBudget::new()).await.unwrap();

    let hidden = row("KA-1");
    assert!(hidden.dismissed, "an Epic arriving is dismissed");
    assert_eq!(hidden.ruled_by.as_deref(), Some("no epics"));
    assert!(!row("KA-2").dismissed, "a rule never hides a taken issue");
    assert_eq!(
        outcome.ruled.iter().map(|r| (r.issue_key.as_str(), r.dismissed.as_deref())).collect::<Vec<_>>(),
        vec![("KA-1", Some("no epics"))]
    );
    let listed: Vec<String> = db.list_active(false).unwrap().into_iter().map(|i| i.issue_key).collect();
    assert_eq!(listed, vec!["KA-2"], "the list no longer shows what the rule hid");
}

#[tokio::test]
#[serial]
async fn dismissal_beats_pin_and_rules_leave_a_dismissed_issue_alone() {
    let _dir = sandbox();
    let server = MockServer::start().await;
    let inbox = configure(
        &server.uri(),
        json!([
            {"name": "hot", "when": {"priority": "High+"}, "then": {"pin": true, "urgent": true}},
            {"name": "no epics", "when": {"type": "Epic"}, "then": {"dismiss": true}}
        ]),
    );

    // KA-5 was dismissed by hand while it was Medium.
    let db = JiraInbox::new().unwrap();
    db.upsert_batch(&[upsert("KA-5", ("Medium", 3), "Bug")]).unwrap();
    db.set_dismissed("KA-5", true).unwrap();

    jira_holds(&server, vec![issue("KA-3", ("High", 2), "Epic"), issue("KA-5", ("Highest", 1), "Bug")]).await;
    let outcome = sync_noninteractive(&inbox, &mut ToastBudget::new()).await.unwrap();

    let epic = row("KA-3");
    assert!(epic.dismissed && !epic.pinned, "a High Epic is hidden, not pinned");
    let ruled = outcome.ruled.iter().find(|r| r.issue_key == "KA-3").unwrap();
    assert_eq!(ruled.urgent, None, "and not announced as urgent");

    let escalated = row("KA-5");
    assert!(escalated.dismissed && !escalated.pinned, "a dismissed issue stays the person's decision");
    assert!(outcome.ruled.iter().all(|r| r.issue_key != "KA-5"));
}

#[tokio::test]
#[serial]
async fn an_urgent_rule_wakes_a_snoozed_issue_and_a_pin_alone_does_not() {
    let _dir = sandbox();
    let server = MockServer::start().await;
    let inbox = configure(
        &server.uri(),
        json!([
            {"name": "hot", "when": {"priority": "High+"}, "then": {"pin": true, "urgent": true}},
            {"name": "pay", "when": {"project": "PAY"}, "then": {"pin": true}}
        ]),
    );

    let db = JiraInbox::new().unwrap();
    db.upsert_batch(&[upsert("KA-4", ("Medium", 3), "Bug"), upsert("PAY-1", ("Low", 4), "Bug")])
        .unwrap();
    let next_week = Local::now().naive_local() + Duration::days(7);
    db.set_snoozed("KA-4", Some(next_week)).unwrap();
    db.set_snoozed("PAY-1", Some(next_week)).unwrap();
    // PAY-1 predates the project being known: a row from before the upgrade.
    kasl::db::db::Db::new()
        .unwrap()
        .conn
        .execute("UPDATE jira_inbox SET project = NULL WHERE issue_key = 'PAY-1'", [])
        .unwrap();

    jira_holds(&server, vec![issue("KA-4", ("High", 2), "Bug"), issue("PAY-1", ("Low", 4), "Bug")]).await;
    let outcome = sync_noninteractive(&inbox, &mut ToastBudget::new()).await.unwrap();

    let woken = row("KA-4");
    assert_eq!(woken.snoozed_until, None, "an urgent rule wakes a sleeping issue");
    assert!(woken.pinned);
    assert!(
        outcome
            .ruled
            .iter()
            .any(|r| r.issue_key == "KA-4" && r.woke && r.urgent.as_deref() == Some("hot"))
    );

    let pinned = row("PAY-1");
    assert!(pinned.pinned, "a project learnt from the poll brings the issue into the rule");
    assert_eq!(pinned.snoozed_until, Some(next_week), "a pin alone leaves the snooze alone");
}

// --- Tags on take ---

#[tokio::test]
#[serial]
async fn a_take_tags_the_task_by_the_rules() {
    let _dir = sandbox();
    configure(
        "https://jira.example.com",
        json!([
            {"when": {"project": "PAY"}, "then": {"tag": "payments"}},
            {"when": {"type": "Incident"}, "then": {"tag": ["fire", "payments"]}}
        ]),
    );
    let db = JiraInbox::new().unwrap();
    db.upsert_batch(&[upsert("PAY-1", ("Medium", 3), "Incident"), upsert("OPS-1", ("Medium", 3), "Story")])
        .unwrap();

    let taken = toast_apply::take("PAY-1", &MoveOptions::unattended()).await.unwrap();
    assert_eq!(
        taken,
        Applied::Taken {
            started: None,
            tags: vec!["payments".to_string(), "fire".to_string()]
        }
    );
    assert!(taken.summary("PAY-1").contains("tagged payments, fire"), "the answer says so");

    let task = &Tasks::new().unwrap().fetch(TaskFilter::ByJiraKey("PAY-1".into())).unwrap()[0];
    let names: Vec<String> = Tags::new()
        .unwrap()
        .get_tags_by_task(task.id.unwrap())
        .unwrap()
        .into_iter()
        .map(|t| t.name)
        .collect();
    assert_eq!(names, vec!["fire", "payments"]);

    let plain = toast_apply::take("OPS-1", &MoveOptions::unattended()).await.unwrap();
    assert_eq!(plain, Applied::Taken { started: None, tags: vec![] }, "no rule, no tags");
}

// --- The pile that was there before the rule ---

#[test]
#[serial]
fn apply_reaches_the_issues_already_there_once() {
    let _dir = sandbox();
    let db = JiraInbox::new().unwrap();
    db.upsert_batch(&[
        upsert("PAY-1", ("High", 2), "Bug"),
        upsert("PAY-2", ("Low", 4), "Epic"),
        upsert("PAY-3", ("Low", 4), "Epic"),
        upsert("OPS-1", ("Low", 4), "Bug"),
    ])
    .unwrap();
    db.set_taken("PAY-3", true).unwrap();
    // A task for the taken one, as `take` made it before the rule existed.
    kasl::db::tasks::Tasks::new()
        .unwrap()
        .insert(&kasl::libs::task::Task::new("PAY-3 Summary", "", Some(0)).from_jira("PAY-3"))
        .unwrap();

    configure(
        "https://jira.example.com",
        json!([
            {"name": "hot", "when": {"priority": "High+"}, "then": {"pin": true, "urgent": true}},
            {"name": "no epics", "when": {"type": "Epic"}, "then": {"dismiss": true}},
            {"when": {"project": "PAY"}, "then": {"tag": "payments"}}
        ]),
    );
    let book = RuleBook::from_config(&db).unwrap();

    let before: Vec<_> = book
        .rules
        .iter()
        .map(|rule| inbox_rules::standing(rule, &db.list(Include::EVERYTHING).unwrap()))
        .collect();
    assert_eq!(
        before[0].waiting + before[1].waiting,
        2,
        "the dry run counts what --apply would change: {before:?}"
    );

    let done = inbox_rules::apply_now(&db, &book).unwrap();
    // Sorted: issues of equal rank come back in no promised order.
    let mut said: Vec<String> = done.iter().map(inbox_rules::sentence).collect();
    said.sort();
    assert_eq!(
        said,
        vec![
            "PAY-1: pinned by 'hot'".to_string(),
            "PAY-2: dismissed by 'no epics'".to_string(),
            "PAY-3: its task tagged payments".to_string(),
        ],
        "pins, hides what is not taken, tags what is - and toasts nothing"
    );
    assert!(!row("PAY-3").dismissed, "the taken Epic stays");

    assert!(inbox_rules::apply_now(&db, &book).unwrap().is_empty(), "a second run has nothing to change");
}

// --- The command line ---

fn kasl(dir: &Path, args: &[&str]) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_kasl"))
        .args(args)
        .env("HOME", dir)
        .env("LOCALAPPDATA", dir)
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .output()
        .expect("kasl runs");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    (out.status.success(), text)
}

#[test]
#[serial]
fn the_rules_command_shows_rules_skips_the_bad_and_applies_on_request() {
    let dir = sandbox();
    let db = JiraInbox::new().unwrap();
    db.upsert_batch(&[upsert("KA-1", ("High", 2), "Bug"), upsert("KA-2", ("Low", 4), "Epic")])
        .unwrap();
    configure(
        "https://jira.example.com",
        json!([
            {"when": {"prority": "High"}, "then": {"dismiss": true}},
            {"name": "hot", "when": {"priority": "High+"}, "then": {"pin": true}},
            {"when": {"type": "Epic"}, "then": {"dismiss": true}}
        ]),
    );

    let (ok, out) = kasl(dir.path(), &["inbox"]);
    assert!(ok, "{out}");
    assert!(
        out.contains("1 inbox rule(s) skipped as unreadable"),
        "the list warns about a skipped rule:\n{out}"
    );

    let (ok, out) = kasl(dir.path(), &["inbox", "rules"]);
    assert!(ok, "{out}");
    assert!(out.contains("Inbox rule 1 is skipped") && out.contains("prority"), "{out}");
    assert!(out.contains("hot") && out.contains("1 not yet pinned"), "{out}");
    assert!(out.contains("--apply"), "a dry run points at --apply:\n{out}");
    assert!(!row("KA-1").pinned, "without --apply nothing changes");

    let (ok, out) = kasl(dir.path(), &["inbox", "rules", "--apply"]);
    assert!(ok, "{out}");
    assert!(out.contains("KA-1: pinned by 'hot'") && out.contains("KA-2: dismissed by 'type Epic'"), "{out}");
    assert!(row("KA-1").pinned && row("KA-2").dismissed);

    let (ok, out) = kasl(dir.path(), &["inbox", "list", "--dismissed"]);
    assert!(ok, "{out}");
    assert!(out.contains("KA-2") && out.contains("ruled out"), "the list shows who hid it:\n{out}");

    let (ok, out) = kasl(dir.path(), &["inbox", "show", "KA-2", "--why"]);
    assert!(ok, "{out}");
    assert!(out.contains("hidden by the rule 'type Epic'"), "{out}");

    let (ok, out) = kasl(dir.path(), &["inbox", "undismiss", "KA-2"]);
    assert!(ok, "{out}");
    let back = row("KA-2");
    assert!(
        !back.dismissed && back.ruled_by.is_none(),
        "undismiss brings it back and takes it from the rule"
    );
}

#[test]
#[serial]
fn type_and_project_cut_the_list() {
    let dir = sandbox();
    JiraInbox::new()
        .unwrap()
        .upsert_batch(&[
            upsert("PAY-1", ("High", 2), "Bug"),
            upsert("PAY-2", ("Low", 4), "Epic"),
            upsert("OPS-1", ("Low", 4), "Epic"),
        ])
        .unwrap();

    let (ok, out) = kasl(dir.path(), &["inbox", "--type", "epic"]);
    assert!(ok, "{out}");
    assert!(out.contains("2 of 3 issues (type epic)") && !out.contains("PAY-1"), "{out}");

    let (ok, out) = kasl(dir.path(), &["inbox", "list", "--type", "Epic", "--project", "PAY", "--project", "XX"]);
    assert!(ok, "{out}");
    assert!(
        out.contains("1 of 3 issues (type Epic, project PAY or XX)") && out.contains("PAY-2") && !out.contains("OPS-1"),
        "{out}"
    );
}

#[test]
#[serial]
fn no_rules_is_said_not_shown_as_an_empty_table() {
    let dir = sandbox();
    let (ok, out) = kasl(dir.path(), &["inbox", "rules"]);
    assert!(ok, "{out}");
    assert!(out.contains("No inbox rules yet"), "{out}");
}
