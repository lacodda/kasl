//! # Kasl - Key Activity Synchronization and Logging
//!
//! A command-line utility for tracking work activities, managing tasks,
//! and generating productivity reports.
//!
//! ## Usage
//!
//! ```rust,no_run
//! fn main() -> std::process::ExitCode {
//!     kasl::run()
//! }
//! ```

pub mod api;
pub mod commands;
pub mod db;
pub mod libs;

/// Runs the CLI: the shared entry point behind both the `kasl` and `ka` binaries.
///
/// Parses the command line before anything else happens, because how the run
/// prints - for a person, `--plain`, `--json` - decides what every later line
/// looks like and where it goes, the debug log included. Then handles the
/// internal `--daemon-run` mode used when the watcher spawns itself, and
/// otherwise dispatches the parsed command.
///
/// The error a command fails with is printed here, marked for the run's style
/// (`❌` for a person, `error:` for `--plain` and `--json`), and turns into
/// exit code 1.
pub fn run() -> std::process::ExitCode {
    libs::output::quiet_on_a_closed_pipe();

    // Clear the binary a previous update left behind; it is only deletable
    // once it is no longer the running image, which is now.
    libs::update::Updater::sweep_backup();

    // Intercepted before clap: this flag is how the background monitoring
    // process is launched, not part of the public command surface.
    let args: Vec<String> = std::env::args().collect();
    let daemon = args.get(1).is_some_and(|arg| arg == "--daemon-run");
    let toast_action = args.get(1).is_some_and(|arg| arg == "toast-action");
    let cli = (!daemon).then(|| commands::Cli::from_args(&args));
    let style = libs::output::init(cli.as_ref().is_some_and(|cli| cli.json()), cli.as_ref().is_some_and(|cli| cli.plain()));

    // Tracing only in debug mode; otherwise log output would clutter normal
    // CLI usage. On stderr, so a debug run of `--json` still prints a
    // document stdout can parse.
    if libs::messages::macros::is_debug_mode() {
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "kasl=debug".into()))
            .with_writer(std::io::stderr)
            .with_ansi(style.colour_stderr)
            .init();
    }

    let outcome = tokio::runtime::Runtime::new().map_err(anyhow::Error::from).and_then(|runtime| {
        runtime.block_on(async {
            let Some(cli) = cli else {
                return commands::watch::run_as_daemon().await;
            };
            // The update notice is for a person at a terminal. A script would
            // wait on GitHub before its answer, a pipe would carry the notice
            // as data, and a cron job would spend the day's one check where
            // nobody reads it. The courier behind a toast button skips it
            // too: it has no terminal, and must not delay the press.
            if !toast_action && !style.json && std::io::IsTerminal::is_terminal(&std::io::stdout()) {
                libs::update::Updater::show_update_notification().await;
            }
            cli.dispatch().await
        })
    });

    match outcome {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            libs::output::report_error(&error);
            std::process::ExitCode::FAILURE
        }
    }
}
