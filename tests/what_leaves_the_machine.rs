//! What kasl sends elsewhere, shown before it goes.
//!
//! Two channels carry this machine's work off it: the team server, and the
//! corporate report API. Each has a command that describes what it sends -
//! `kasl server manifest` for the first, `kasl report --send --show` for the
//! second - and the promise of both is the same one: what is printed is what
//! would be sent.
//!
//! That promise is only worth making if a preview cannot drift from the
//! request. These tests spawn the real binary against a private data
//! directory, so what they read is what a person reads, and they check the
//! two ways a preview goes wrong: showing less than is sent, and sending
//! while claiming only to show.

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use std::path::Path;
    use std::process::{Command, Output, Stdio};
    use tempfile::TempDir;

    /// A kasl command bound to a private data directory, with no stdin.
    fn kasl_cmd(dir: &Path) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_kasl"));
        cmd.env("HOME", dir).env("LOCALAPPDATA", dir).stdin(Stdio::null());
        cmd
    }

    fn stdout_of(out: &Output) -> String {
        String::from_utf8_lossy(&out.stdout).to_string()
    }

    /// The data directory kasl itself would use under `dir`.
    ///
    /// Asked of `DataStorage` rather than spelled out, because the layout
    /// differs per platform - `LOCALAPPDATA/lacodda/kasl` on Windows,
    /// `~/.local/share/lacodda/kasl` on Linux, `~/Library/Application
    /// Support/lacodda/kasl` on macOS. Writing one of those by hand makes a
    /// test that passes on the machine it was written on and fails on the
    /// other two, which is exactly what it did.
    fn data_dir(dir: &Path) -> std::path::PathBuf {
        // SAFETY: these tests are #[serial]
        unsafe {
            std::env::set_var("HOME", dir);
            std::env::set_var("LOCALAPPDATA", dir);
        }
        kasl::libs::data_storage::DataStorage::new()
            .get_path("config.json")
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf()
    }

    /// Writes a config naming a corporate API that does not exist.
    ///
    /// The address is unroutable on purpose. `--show` must not need it, and a
    /// preview that reached the network would hang here rather than print -
    /// which is the point: the failure is loud instead of invisible.
    fn configure_si(dir: &Path) {
        write_config(
            dir,
            serde_json::json!({
                "si": {
                    "login": "employee",
                    "auth_url": "http://127.0.0.1:1",
                    "api_url": "http://127.0.0.1:1/api"
                }
            }),
        );
    }

    /// Writes `config` into the data directory kasl reads under `dir`.
    fn write_config(dir: &Path, config: serde_json::Value) {
        let config_dir = data_dir(dir);
        std::fs::create_dir_all(&config_dir).unwrap();
        std::fs::write(config_dir.join("config.json"), serde_json::to_string_pretty(&config).unwrap()).unwrap();
    }

    /// Starts today with one task on it, so there is a report to describe.
    fn seed_a_day_with_a_task(dir: &Path) {
        // SAFETY: these tests are #[serial]
        unsafe {
            std::env::set_var("HOME", dir);
            std::env::set_var("LOCALAPPDATA", dir);
        }
        let today = chrono::Local::now().date_naive();
        let mut workdays = kasl::db::workdays::Workdays::new().unwrap();
        workdays.insert_start(today).unwrap();
        // Backdated so the open day holds a work interval long enough to
        // survive the short-interval filter. A day started this second is
        // filtered down to nothing, and the payload would then be empty for a
        // reason that has nothing to do with what this test is about.
        workdays
            .update_start(today, (chrono::Local::now() - chrono::Duration::hours(4)).naive_local())
            .unwrap();

        let out = kasl_cmd(dir)
            .args(["task", "add", "--name", "Wire up the manifest", "--completeness", "60"])
            .output()
            .unwrap();
        assert!(out.status.success(), "seeding a task failed: {}", String::from_utf8_lossy(&out.stderr));
    }

    #[serial]
    #[test]
    fn the_harness_writes_the_config_where_kasl_reads_it() {
        // This ran green on Windows and red on Linux and macOS, because the
        // helper spelled out `LOCALAPPDATA/lacodda/kasl` while kasl looks
        // under `~/.local/share` and `~/Library/Application Support` there.
        // Every other test in this file then exercised the "no SiServer
        // configured" path and asserted nothing it meant to.
        //
        // Checked by the observable consequence rather than by comparing
        // paths: a config that kasl cannot find produces a different answer,
        // and that is the difference worth failing on.
        let dir = TempDir::new().unwrap();
        configure_si(dir.path());
        seed_a_day_with_a_task(dir.path());

        let out = kasl_cmd(dir.path()).args(["report", "--send", "--show"]).output().unwrap();
        let complaint = String::from_utf8_lossy(&out.stderr);

        assert!(
            !complaint.contains("SiServer configuration not found"),
            "kasl did not find the config this test wrote - the layout differs on this platform:
{complaint}"
        );
    }

    #[serial]
    #[test]
    fn show_names_every_field_of_the_form_that_would_be_posted() {
        let dir = TempDir::new().unwrap();
        configure_si(dir.path());
        seed_a_day_with_a_task(dir.path());

        let out = kasl_cmd(dir.path()).args(["report", "--send", "--show"]).output().unwrap();
        assert!(out.status.success(), "--show failed: {}", String::from_utf8_lossy(&out.stderr));
        let shown = stdout_of(&out);

        // Every field the request carries, not only the interesting one. A
        // field sent but not shown is a field the employee was not told
        // about, which is the whole failure this command exists to prevent.
        for field in ["date", "tasks", "comment", "day_type", "duty", "only_save"] {
            assert!(shown.contains(field), "the preview leaves out the '{field}' field:\n{shown}");
        }

        // And where it is going. Half of "what leaves this machine" is which
        // machine it reaches.
        assert!(
            shown.contains("127.0.0.1:1/api/report-card/send-daily-report"),
            "the preview does not name the address it would post to:\n{shown}"
        );

        // The work itself, in the words the request would carry it in.
        assert!(shown.contains("Wire up the manifest"), "the task is missing from the preview:\n{shown}");
    }

    #[serial]
    #[test]
    fn show_sends_nothing_and_ends_no_day() {
        let dir = TempDir::new().unwrap();
        configure_si(dir.path());
        seed_a_day_with_a_task(dir.path());

        let out = kasl_cmd(dir.path()).args(["report", "--send", "--show"]).output().unwrap();
        assert!(out.status.success(), "--show failed: {}", String::from_utf8_lossy(&out.stderr));

        // `--send` finalizes the day before it assembles anything. A preview
        // that did the same would quietly end someone's working day for
        // asking what would happen if they ended it - and the day would stay
        // ended after they decided not to.
        // SAFETY: these tests are #[serial]
        unsafe {
            std::env::set_var("HOME", dir.path());
            std::env::set_var("LOCALAPPDATA", dir.path());
        }
        let today = chrono::Local::now().date_naive();
        let workday = kasl::db::workdays::Workdays::new().unwrap().fetch(today).unwrap().unwrap();
        assert!(workday.end.is_none(), "--show ended the day it was only asked to describe");

        // Nothing was posted either. The configured address has no listener,
        // so a request would have failed and said so; a clean run that
        // mentions no failure is a run that did not reach the network.
        let shown = stdout_of(&out);
        assert!(!shown.contains("Report sent"), "the preview claims something was sent:\n{shown}");
    }

    #[serial]
    #[test]
    fn show_describes_a_day_that_is_below_the_productivity_threshold() {
        let dir = TempDir::new().unwrap();
        configure_si(dir.path());
        seed_a_day_with_a_task(dir.path());

        let out = kasl_cmd(dir.path()).args(["report", "--send", "--show"]).output().unwrap();

        // The threshold decides whether a report may be submitted, not what a
        // submission contains. Refusing to describe a day it would block
        // would hide exactly the day someone opens this command to look at.
        assert!(out.status.success(), "--show refused a day: {}", String::from_utf8_lossy(&out.stderr));
        assert!(stdout_of(&out).contains("only_save"), "the payload was not described:\n{}", stdout_of(&out));
    }

    #[serial]
    #[test]
    fn show_without_send_is_refused_rather_than_guessed_at() {
        let dir = TempDir::new().unwrap();
        configure_si(dir.path());

        let out = kasl_cmd(dir.path()).args(["report", "--show"]).output().unwrap();

        // `--show` describes the sending, so there is nothing for it to mean
        // on its own. Accepting it would leave someone believing they had
        // asked about the payload when they had asked for the daily report.
        assert!(!out.status.success(), "--show alone should not be accepted");
        let complaint = String::from_utf8_lossy(&out.stderr);
        assert!(complaint.contains("--send"), "the refusal should name the flag that is missing:\n{complaint}");
    }

    #[serial]
    #[test]
    fn show_is_not_accepted_for_the_monthly_report() {
        let dir = TempDir::new().unwrap();
        configure_si(dir.path());

        let out = kasl_cmd(dir.path()).args(["report", "--send", "--show", "--month"]).output().unwrap();

        // The monthly report posts a date and nothing else, so there is no
        // payload to inspect. Accepting the flag would answer a question
        // about the daily payload with silence - or, worse, submit the month
        // while appearing to preview it.
        assert!(!out.status.success(), "--show --month should not be accepted");
        let complaint = String::from_utf8_lossy(&out.stderr);
        assert!(complaint.contains("--month"), "the refusal should name the conflict:\n{complaint}");
    }

    /// The field names `--show` printed, in the order it printed them.
    ///
    /// Parsed back out of the preview rather than taken from the code, so
    /// what is compared is what a person reads.
    fn fields_named_in(preview: &str) -> Vec<String> {
        preview
            .lines()
            .filter(|line| line.starts_with("  ") && !line.starts_with("   "))
            .filter_map(|line| line.trim().split(':').next().map(str::to_string))
            .filter(|name| !name.is_empty())
            .collect()
    }

    #[serial]
    #[tokio::test]
    async fn the_preview_names_exactly_the_fields_the_request_carries() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let dir = TempDir::new().unwrap();
        let server = MockServer::start().await;

        // A config pointed at the mock, and a cached session so `send`
        // reaches the request without a keyring or a prompt.
        write_config(
            dir.path(),
            serde_json::json!({
                "si": { "login": "employee", "auth_url": server.uri(), "api_url": server.uri() }
            }),
        );
        std::fs::write(data_dir(dir.path()).join(".si_session_id"), "cached-session").unwrap();

        seed_a_day_with_a_task(dir.path());

        Mock::given(method("POST"))
            .and(path("/report-card/send-daily-report"))
            .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
            .mount(&server)
            .await;

        let shown = stdout_of(&kasl_cmd(dir.path()).args(["report", "--send", "--show"]).output().unwrap());
        let sent = kasl_cmd(dir.path()).args(["report", "--send"]).output().unwrap();
        assert!(sent.status.success(), "--send failed: {}", String::from_utf8_lossy(&sent.stderr));

        // The body the server actually received, as multipart puts it on the
        // wire: every part announces its name in a Content-Disposition header.
        let requests = server.received_requests().await.unwrap();
        let posted = requests
            .iter()
            .find(|request| request.url.path() == "/report-card/send-daily-report")
            .expect("the daily report was never posted");
        let body = String::from_utf8_lossy(&posted.body);
        let posted_fields: Vec<String> = body
            .match_indices("name=\"")
            .filter_map(|(at, _)| body[at + 6..].split('"').next().map(str::to_string))
            .collect();

        // This is the promise the command makes, checked against the request
        // rather than against the code that builds it: a field added to the
        // form and not to the preview is a field sent without being shown,
        // which is the one way this feature can quietly become a lie.
        assert_eq!(
            fields_named_in(&shown),
            posted_fields,
            "the preview and the request disagree about what is sent
preview:
{shown}
posted: {posted_fields:?}"
        );
    }

    #[serial]
    #[test]
    fn the_manifest_needs_a_connected_server_and_says_so() {
        let dir = TempDir::new().unwrap();

        let out = kasl_cmd(dir.path()).args(["server", "manifest"]).output().unwrap();

        // Nothing to describe without a server, and the fix is the same one
        // every other server subcommand names.
        assert!(!out.status.success(), "a manifest with no server should fail");
        let complaint = format!("{}{}", stdout_of(&out), String::from_utf8_lossy(&out.stderr));
        assert!(
            complaint.contains("not connected to a kasl-server"),
            "the refusal should say there is no server:\n{complaint}"
        );
    }
}
