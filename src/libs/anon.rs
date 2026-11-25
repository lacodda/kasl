//! `--anon`: the same output with every word that is yours replaced.
//!
//! A transcript for a README, a bug report or a screenshot is taken from a
//! live installation, and a live installation holds the names of real work:
//! task names, comments, issue keys, the tracker's address. With `--anon` a
//! reading command prints its usual output - same rows, same hours, same
//! percentages, same statuses - with each of those replaced by a stand-in.
//!
//! What is replaced is the text a person or their tracker wrote: a task's
//! name and comment, a tag's and a template's name, an issue's key, summary,
//! project and address, the name of an inbox rule. What stays is what kasl
//! counted and the vocabulary a tracker shares with every other (`High`,
//! `In Progress`, `Bug`).
//!
//! A stand-in is derived from the text and a salt this installation keeps,
//! so one task carries one stand-in in every command and on every day - a
//! transcript of `kasl task list` and one of `kasl report` agree - and
//! nobody without the salt can try names against it.
//!
//! The replacement happens where records are turned into output (the table
//! views, the JSON documents, the standup's markdown), never where they are
//! read: filters, orders and sums work on the real values, and nothing a
//! command writes back can carry a stand-in. Only commands that write nothing
//! take the flag at all - see `Cli::dispatch`.

use crate::db::inbox::InboxItem;
use crate::db::tags::Tag;
use crate::db::templates::TaskTemplate;
use crate::libs::data_storage::DataStorage;
use crate::libs::task::Task;
use anyhow::{Context, Result};
use std::borrow::Cow;
use std::sync::OnceLock;

/// The file in the data directory that holds this installation's salt.
const SALT_FILE: &str = "anon.salt";

/// The mask of this run; `None` when `--anon` was not asked for.
static MASK: OnceLock<Option<Mask>> = OnceLock::new();

/// Turns masking on for this run, reading the installation's salt - made on first use.
///
/// Called once, before anything prints. A salt that cannot be read or kept is
/// an error: a run that was asked to hide names must not print them because a
/// file was in the way.
pub fn init(on: bool) -> Result<()> {
    let mask = if on { Some(Mask::new(salt()?)) } else { None };
    // The first call wins, as with the output style.
    let _ = MASK.set(mask);
    Ok(())
}

/// Whether this run masks what it prints.
pub fn on() -> bool {
    mask().is_some()
}

/// The mask of this run, for output that is built from several records at once.
pub fn current() -> Option<&'static Mask> {
    mask()
}

fn mask() -> Option<&'static Mask> {
    MASK.get().and_then(Option::as_ref)
}

/// The installation's salt: read from the data directory, written there the first time.
fn salt() -> Result<u64> {
    let path = DataStorage::new().get_path(SALT_FILE)?;
    if let Ok(text) = std::fs::read_to_string(&path)
        && let Ok(salt) = u64::from_str_radix(text.trim(), 16)
    {
        return Ok(salt);
    }
    // `RandomState` is seeded by the OS for every process: randomness
    // without a dependency taken for sixteen hex digits.
    use std::hash::{BuildHasher, Hasher};
    let salt = std::collections::hash_map::RandomState::new().build_hasher().finish();
    std::fs::write(&path, format!("{salt:016x}\n")).with_context(|| format!("cannot keep the salt --anon needs in {}", path.display()))?;
    Ok(salt)
}

/// Stand-ins for one installation's words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mask {
    salt: u64,
}

impl Mask {
    pub fn new(salt: u64) -> Self {
        Mask { salt }
    }

    /// A number that depends on the salt, the kind of thing and its text.
    ///
    /// FNV-1a with a finishing mix, written out here because the stand-ins
    /// must not change with the compiler: `DefaultHasher` promises nothing
    /// across versions, and a transcript taken after an update would name
    /// every task anew.
    fn number(&self, kind: &str, text: &str) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325_u64 ^ self.salt;
        for byte in kind.bytes().chain([0]).chain(text.bytes()) {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
        }
        hash ^= hash >> 33;
        hash = hash.wrapping_mul(0xff51_afd7_ed55_8ccd);
        hash ^= hash >> 33;
        hash = hash.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
        hash ^ (hash >> 33)
    }

    /// `Task 4f2a1c`: a word and six hex digits; nothing for nothing.
    pub fn label(&self, word: &str, text: &str) -> String {
        if text.trim().is_empty() {
            return String::new();
        }
        format!("{word} {:06x}", self.number(word, text) & 0xff_ffff)
    }

    /// `tag-4f2a1c`: the form for names typed on a command line.
    pub fn slug(&self, word: &str, text: &str) -> String {
        if text.trim().is_empty() {
            return String::new();
        }
        format!("{word}-{:06x}", self.number(word, text) & 0xff_ffff)
    }

    /// A project: three letters for a Jira key (`PAY` becomes `QXT`), a slug for a path.
    pub fn project(&self, project: &str) -> String {
        if project.is_empty() {
            return String::new();
        }
        if !project.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()) {
            return self.slug("project", project);
        }
        let mut number = self.number("project", project);
        (0..3)
            .map(|_| {
                let letter = (b'A' + (number % 26) as u8) as char;
                number /= 26;
                letter
            })
            .collect()
    }

    /// An issue key in the shape it had: `PAY-482` becomes `QXT-1731`,
    /// `group/app#14` becomes `project-4f2a1c#902`.
    ///
    /// The project part is [`Mask::project`], so a key and the project
    /// column beside it still agree.
    pub fn key(&self, key: &str) -> String {
        if key.is_empty() {
            return String::new();
        }
        let number = 1 + self.number("key", key) % 9999;
        match key.rfind(['-', '#', '!']) {
            Some(at) if at > 0 && at + 1 < key.len() && key[at + 1..].chars().all(|c| c.is_ascii_digit()) => {
                format!("{}{}{number}", self.project(&key[..at]), &key[at..=at])
            }
            _ => format!("REF-{number}"),
        }
    }

    /// A task's name: its stand-in, behind its issue's stand-in key when the name opens with the key.
    ///
    /// `kasl inbox take` opens a task's name with the issue key, and the
    /// tables and the standup rely on finding it there.
    pub fn task_name(&self, name: &str, issue_key: Option<&str>) -> String {
        let opening = issue_key
            .filter(|key| name.starts_with(key))
            .or_else(|| name.split_whitespace().next().filter(|word| looks_like_a_jira_key(word)));
        match opening {
            Some(key) => {
                let rest = name[key.len()..].trim();
                [self.key(key), self.label("Task", rest)]
                    .into_iter()
                    .filter(|part| !part.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ")
            }
            None => self.label("Task", name),
        }
    }

    /// Where a stand-in issue lives: a host no one owns (RFC 2606).
    pub fn url(&self, url: &str, key: &str) -> String {
        if url.is_empty() {
            return String::new();
        }
        format!("https://tracker.example/{}", self.key(key))
    }

    pub fn task(&self, task: &Task) -> Task {
        Task {
            name: self.task_name(&task.name, task.issue_key.as_deref()),
            comment: self.label("Note", &task.comment),
            tags: task.tags.iter().map(|tag| self.tag(tag)).collect(),
            issue_key: task.issue_key.as_deref().map(|key| self.key(key)),
            ..task.clone()
        }
    }

    pub fn tag(&self, tag: &Tag) -> Tag {
        Tag {
            name: self.slug("tag", &tag.name),
            ..tag.clone()
        }
    }

    pub fn template(&self, template: &TaskTemplate) -> TaskTemplate {
        TaskTemplate {
            name: self.slug("template", &template.name),
            task_name: self.task_name(&template.task_name, None),
            comment: self.label("Note", &template.comment),
            ..template.clone()
        }
    }

    pub fn issue(&self, item: &InboxItem) -> InboxItem {
        InboxItem {
            issue_key: self.key(&item.issue_key),
            summary: self.label("Issue", &item.summary),
            url: self.url(&item.url, &item.issue_key),
            project: item.project.as_deref().map(|project| self.project(project)),
            ruled_by: item.ruled_by.as_deref().map(|rule| self.slug("rule", rule)),
            ..item.clone()
        }
    }
}

/// `PROJ-12`: capitals and digits, a hyphen, digits.
fn looks_like_a_jira_key(word: &str) -> bool {
    word.split_once('-').is_some_and(|(project, number)| {
        project.len() >= 2
            && project.starts_with(|c: char| c.is_ascii_uppercase())
            && project.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
            && !number.is_empty()
            && number.chars().all(|c| c.is_ascii_digit())
    })
}

/// A task as this run prints it.
pub fn task(task: &Task) -> Cow<'_, Task> {
    match mask() {
        Some(mask) => Cow::Owned(mask.task(task)),
        None => Cow::Borrowed(task),
    }
}

/// Tasks as this run prints them.
pub fn tasks(tasks: &[Task]) -> Cow<'_, [Task]> {
    match mask() {
        Some(mask) => Cow::Owned(tasks.iter().map(|task| mask.task(task)).collect()),
        None => Cow::Borrowed(tasks),
    }
}

/// A tag as this run prints it.
pub fn tag(tag: &Tag) -> Cow<'_, Tag> {
    match mask() {
        Some(mask) => Cow::Owned(mask.tag(tag)),
        None => Cow::Borrowed(tag),
    }
}

/// Tags as this run prints them.
pub fn tags(tags: &[Tag]) -> Cow<'_, [Tag]> {
    match mask() {
        Some(mask) => Cow::Owned(tags.iter().map(|tag| mask.tag(tag)).collect()),
        None => Cow::Borrowed(tags),
    }
}

/// A template as this run prints it.
pub fn template(template: &TaskTemplate) -> Cow<'_, TaskTemplate> {
    match mask() {
        Some(mask) => Cow::Owned(mask.template(template)),
        None => Cow::Borrowed(template),
    }
}

/// Templates as this run prints them.
pub fn templates(templates: &[TaskTemplate]) -> Cow<'_, [TaskTemplate]> {
    match mask() {
        Some(mask) => Cow::Owned(templates.iter().map(|template| mask.template(template)).collect()),
        None => Cow::Borrowed(templates),
    }
}

/// An inbox issue as this run prints it.
pub fn issue(item: &InboxItem) -> Cow<'_, InboxItem> {
    match mask() {
        Some(mask) => Cow::Owned(mask.issue(item)),
        None => Cow::Borrowed(item),
    }
}

/// Inbox issues as this run prints them.
pub fn issues(items: &[InboxItem]) -> Cow<'_, [InboxItem]> {
    match mask() {
        Some(mask) => Cow::Owned(items.iter().map(|item| mask.issue(item)).collect()),
        None => Cow::Borrowed(items),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MASK: Mask = Mask { salt: 0x5eed };

    #[test]
    fn a_stand_in_is_the_same_every_time_and_another_under_another_salt() {
        assert_eq!(MASK.label("Task", "Fix the login redirect"), MASK.label("Task", "Fix the login redirect"));
        assert_ne!(MASK.label("Task", "Fix the login redirect"), MASK.label("Task", "Fix the logout redirect"));
        assert_ne!(
            MASK.label("Task", "Fix the login redirect"),
            Mask::new(0xbeef).label("Task", "Fix the login redirect")
        );
    }

    #[test]
    fn nothing_stays_nothing() {
        assert_eq!(MASK.label("Note", "  "), "");
        assert_eq!(MASK.key(""), "");
        assert_eq!(MASK.url("", "PAY-1"), "");
    }

    #[test]
    fn a_jira_key_keeps_its_shape_and_agrees_with_its_project() {
        let key = MASK.key("PAY-482");
        let (project, number) = key.split_once('-').expect("a hyphen");
        assert_eq!(project, MASK.project("PAY"));
        assert_eq!(project.len(), 3);
        assert!(project.chars().all(|c| c.is_ascii_uppercase()));
        assert!(number.parse::<u32>().is_ok_and(|n| (1..=9999).contains(&n)));
        assert!(!key.contains("PAY") && !key.contains("482"), "{key}");
    }

    #[test]
    fn a_gitlab_reference_keeps_its_mark() {
        let key = MASK.key("group/app#14");
        assert!(key.starts_with("project-") && key.contains('#'), "{key}");
        assert!(!key.contains("group") && !key.contains("app#"), "{key}");
        assert!(MASK.key("no number here").starts_with("REF-"));
    }

    #[test]
    fn a_task_name_keeps_its_key_in_front_and_loses_its_words() {
        let name = MASK.task_name("PAY-482 Refund rounds the wrong way", Some("PAY-482"));
        assert!(name.starts_with(&MASK.key("PAY-482")), "{name}");
        assert!(name.contains(" Task "), "{name}");
        assert!(!name.contains("Refund") && !name.contains("PAY"), "{name}");

        // A name typed by hand that opens with a key is read the same way.
        let typed = MASK.task_name("PAY-482 Refund rounds the wrong way", None);
        assert_eq!(typed, name);

        // A name that is only the key has no words to stand in for.
        assert_eq!(MASK.task_name("PAY-482", Some("PAY-482")), MASK.key("PAY-482"));
        assert!(MASK.task_name("Write the release notes", None).starts_with("Task "));
    }

    #[test]
    fn a_task_loses_every_word_that_was_written() {
        let mut task = Task::new("PAY-482 Refund rounds the wrong way", "ask Marta about cents", Some(40)).from_issue("PAY-482");
        task.tags = vec![Tag::new("payments".to_string(), None)];
        let masked = MASK.task(&task);
        let printed = format!("{masked:?}");
        for word in ["PAY", "Refund", "Marta", "payments"] {
            assert!(!printed.contains(word), "{word} survived: {printed}");
        }
        assert_eq!(masked.completeness, Some(40));
    }
}
