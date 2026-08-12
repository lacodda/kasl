# Changelog

All notable changes to this project are documented in this file.

## [1.0.3] - 2026-08-12

### Bug Fixes
- Declare the MSRV that actually builds
- Make the installer work at all, and add one for Windows

### Documentation
- Title the init page like every other command page

### Features
- Tier the exe icon by level and show the logo on toasts
## [1.0.2] - 2026-08-09

### Bug Fixes
- One README and one description across all three pages
## [1.0.1] - 2026-08-09

### Bug Fixes
- Let `task add --name` run without a terminal

### Documentation
- Rewrite in the turnout style
- Bring the landing in line with turnout
- Use the L tile for the header mark
## [1.0.0] - 2026-08-09

### Breaking Changes
- **Replace manual breaks with user-stated pauses** - `kasl breaks` is removed; use `kasl pauses add`. The productivity config no longer accepts min_break_duration or max_break_duration.
- **Move task, tag, template and inbox to subcommands** - task, tag, template and inbox no longer accept their old flags. Use the subcommands listed above.
- **Move credentials to the OS keyring** - ENCRYPTION_KEY and ENCRYPTION_IV are no longer used at build time. Credentials no longer travel with a copy of the data directory; they are entered once per machine.
- **Default report language to English** - reports render in English by default. Set `report.language` to "ru" to keep Russian output.

### Bug Fixes
- Report the binary name kasl instead of the crate name kasl-cli
- Download the binary on first run, not only in postinstall
- Find completed issues without status-name JQL
- Expand push ranges and keep only today's own commits
- Close an unclosed past workday at its last activity
- Import msg_info outside Windows too
- Enable the systemd unit by symlink, not systemctl

### Documentation
- Repair doc examples and gate them in CI
- Move the site from mdBook to Astro Starlight

### Features
- Replace manual breaks with user-stated pauses
- Add shell completion script generation
- Move task, tag, template and inbox to subcommands
- Move credentials to the OS keyring
- Implement autostart on macOS and Linux
- Default report language to English
- Add `ka` as a short alias for the kasl binary

### Testing
- Poll for daemon state instead of fixed sleeps
- Skip keyring assertions where no keyring exists

### style
- Rustfmt and clippy cleanup for gitlab and jira changes
## [0.10.1] - 2026-08-07

### Bug Fixes
- Gate Windows-only imports and macOS-incompatible toast actions
- Silence clippy on never-compiled unix paths
- Unix branch of autostart disable test referenced a renamed binding
- Use assert! instead of assert_eq! with a bool literal
- Make watch --stop idempotent when the process is already dead
- Tolerate the PID file vanishing while stop runs

### CI
- Make all three OSes a strict gate
- Build all three OSes and generate notes with git-cliff

### Features
- Download Linux and macOS binaries

### Testing
- Capture stop output for CI diagnostics
## [0.10.0] - 2026-08-07

### Bug Fixes
- Rename package to kasl-cli
- Date filters no longer shift a day in negative UTC offsets
- Replace removed before_exec with pre_exec on Unix

### Build
- Publish crate as kasl-cli keeping the kasl binary identity
- Upgrade to Rust 1.97, edition 2024, and latest dependencies
- Adapt to the Rust 1.97 toolchain and reqwest 0.13

### CI
- Publish to crates.io and npm via OIDC trusted publishing
- Add fmt/clippy/test workflow, repair docs deploy, refresh README

### Documentation
- Document the inbox command

### Features
- Add npm installer package
- Make completed-issue statuses configurable
- Add Jira inbox polling, CLI, and clickable toasts
- Add Scoring sort fields, status ids, and full search pagination
- Add --limit and truncate summary to terminal width

### Testing
- Serialize env-dependent tests and harden the daemon harness
## [0.9.0] - 2026-08-07

### Bug Fixes
- Merge adjacent pauses before duration filtering
- Use a dedicated small gap threshold for merging pauses
- Distribute hourly report tasks one per hour slot
- Keep task table within terminal width
- Sanitize multi-line pasted task names
- Exclude pre-workday idle from pause accounting

### Features
- Add hourly daily report with localization and design templates
- Unify task discovery into a single filtered MultiSelect
- Add configurable ignore list for task discovery
## [0.8.2] - 2025-08-24

### Bug Fixes
- Reset retry counter after successful API authentication
- Integrate breaks into report calculations and API submissions
- Improve PATH configuration error handling and messaging

### Documentation
- Standardize documentation across entire codebase

### Features
- Add ProductivityConfig setup to init command
- Improve reliability and update handling
- Auto-restart watcher after configuration changes in init

### Refactoring
- Remove dead code including PauseGroup trait and associated
- Fix productivity calculation and improve daemon tests
- Consolidate productivity calculations and improve breaks integration
- The logic of the report_with_intervals and report methods in the View module have been combined into a single report method
- Productivity module refactoring
- Remove all compatibility functions, pure struct API

### Testing
- Move inline tests from src/libs/report.rs to tests/report_functions.rs
- Re-enable tests and fix related warnings
## [0.8.1] - 2025-08-22

### Documentation
- Update report command documentation for new interval filtering
- Update documentation for new breaks command and productivity features

### Features
- Implement productivity-focused breaks system to replace adjust command

### Refactoring
- Replace short interval database cleanup with display filtering

### Testing
- Implement missing test coverage for critical components
- Add comprehensive test suite for breaks functionality and productivity features
## [0.8.0] - 2025-08-20

### Bug Fixes
- Fix task template format for submission
- Fix errors in tests
- Create base tables in migration v1 before indexes
- Ensure that the configuration is completely written to disk
- Make ensure_workday_started available for tests
- Use correct field for task ID filtering

### Documentation
- Update README
- Add comprehensive rustdoc comments throughout the codebase
- Add comprehensive comments to core project files
- Add comprehensive comments to command module and core commands
- Add comprehensive comments to monitoring and time adjustment commands
- Add comprehensive comments to report and export commands
- Add comprehensive comments to task management and template commands
- Add comprehensive comments to tag management and migration commands
- Add comprehensive comments to API integration modules
- Add comprehensive comments to DB modules
- Add detailed comments to several Libs modules
- Add comprehensive comments to task lib module
- Add detailed comments to several Libs modules
- Add detailed comments to several Libs modules
- Complete command documentation with real output examples

### Features
- Add conditional structured logging with debug mode support
- Implement Windows autostart via Task Scheduler
- Add task delete command with confirmation
- Add task edit command with interactive mode
- Add short work intervals detection and removal
- Add time adjustment command with preview
- Add database migration system with automatic upgrades
- Add report export to CSV, JSON and Excel formats
- Add task templates for frequently used tasks
- Add tagging system for task categorization

### Testing
- Add comprehensive tests for new features and fix existing tests

### style
- Unify code comment formatting across all source files
## [0.6.0] - 2024-08-01

### Bug Fixes
- Added message that update is not required if the latest version is used
- Fixed a bug with password request after session expires in Jira
- Fixed bug with requesting unavailable Jira API
- Refine activity detection with rdev
- Improve network error handling to prevent crashes
- Align sent payload with displayed report logic
- Correct visibility and imports to fix build errors
- Adjust pause start time by threshold value
- Fix Windows process termination and refactor daemon logic
- Embed encryption keys at compile time

### Features
- Added Secret module for encryption and decryption of passwords to services
- Secret module functionality added to Jira service
- Secret module functionality added to Si service
- Added update checking functionality and a command to update the application to the latest version
- Add optional --month flag to generate report for current month
- Add activity monitoring daemon with enigo
- Add breaks table and CRUD operations
- Implement real activity detection
- Add kasl breaks command
- Add break monitoring and server configuration
- Add workdays table with basic CRUD operations
- Automate workday start detection in Monitor based on activity threshold
- Refactor 'sum' command to use workday/break model
- Refactor report command and remove events system
- Centralize formatting logic and improve documentation
- Implement background daemon mode
- Add total pause time calculation and display
- Add work productivity calculation and display
- Implement automatic restart on new instance
- Add application icon for Windows
- Add anyhow for better error handling
- Add centralized messages system
- Implement centralized messaging system and begin migration

### Refactoring
- Common methods are moved to the Session trait
- Improved functionality of build.rs
- Remove system event-based scheduler
- Change start and end to TIMESTAMP in breaks table
- Refactor state management and activity detection
- Improve report command and table output
- Rename 'Break' entity to 'Pause'
- Refactor update logic and improve command structure
- Migrate database modules to anyhow
- Migrate entire codebase to anyhow for better error handling
- Migrate api and commands to new messaging system
- Migrate all messages to new messaging system

### Testing
- Add tests for config
- Tests for report have been fixed
## [0.5.0] - 2024-07-11

### Bug Fixes
- The current day is excluded from the sum command's calculation of monthly statistics

### Features
- Improved view of the list of issues proposed for adding (divided into groups: Incomplete, Gitlab, Jira, etc.)
- Tasks are divided into groups for even distribution in the report
## [0.4.1] - 2024-06-30

### Bug Fixes
- Removed information about commit ID from Gitlab

### Features
- Added a function for sending a monthly summary report in the Si module
## [0.4.0] - 2024-06-25

### Bug Fixes
- Fixed a bug in searching tasks by ID

### Features
- Added functionality to create a report for a specific date
- Added Jira API module
- Added tasks from Jira to fill the list of daily tasks

### Refactoring
- API module configs have been moved from the libs/config.rs file to the corresponding module files
## [0.3.0] - 2024-06-18

### Bug Fixes
- Fixed a bug with receiving commits from Gitlab

### Features
- Added loading of rest days for the correct operation of the sum command
- Added average operating time to the sum command report
- Added gitlab api module
- Added commits from gitlab to create a list of tasks
## [0.2.0] - 2024-05-17

### Bug Fixes
- Fixed a bug in calculating time using the "sum" command. Optimization of the events db module.

### Features
- Added "raw" flag to the "event" command
## [0.1.1] - 2024-04-08

### Bug Fixes
- The path to SESSION_ID_FILE has been corrected in the delete_session_id method
- The service println has been removed
- Fixed an error in calculating time if the interval end timestamp is missed

### Features
- Watch command has been added
- Added aliases for event command
## [0.1.0] - 2024-03-25

### Bug Fixes
- Unused commands have been removed
- The final daily event has been added after submitting report

### Features
- A simple wizard has been added to set configuration settings
- Summary command has been added
## [0.0.2] - 2024-03-19

### Bug Fixes
- The path to the .session_id file has been replaced with the path in DataStorage

### Features
- Bash installation script has been slightly modified
## [0.0.1] - 2024-03-18

### Bug Fixes
- Solved the error: "recursion in an `async fn` requires boxing"
- Renaming paths in kasl
- The input field has been replaced with a password entry
- The formation of the final report for sending has been changed

### Documentation
- Files added: README.md, LICENSE, rellr.json
- Kasl logo added
- A detailed user guide has been created
- Readmes and introductory documentation have been updated

### Features
- Added basic commands
- The "rusqlite" library has been added to the project
- The "insert_event" method has been added to the Db module
- Task structure, tasks scheme and "insert_task" method has been added
- The "fetch" method has been added to the Tasks DB module
- The "excluded_from_search" and "task_id" fields have been added to the Task structure
- The "dialoguer" library been added to the project
- Finding unfinished tasks has recently been added as an option to the "task" command
- Data output in tabular form has been added to the task module
- Scheduler module has been added to the project
- The function for deleting tasks from the Windows Scheduler has been added to the project
- Events module has been updated
- Scheduler module has been updated
- The function of counting and displaying working time tables has been addded to the events module
- Working hours have been added to the events module
- Http module has been added
- Config module has been added
- Report command has been added
- Init command has been updated
- Report command has been updated
- The Http module was renamed to Si and moved to the api directory. Methods for working with the session storage file have also been added. The application configuration has been changed to accommodate the new requirements.
- Bash installation script has been added
- Created directory structure in AppData for database, user files and configuration.
- Bash installation script has been moved to the tools directory

### Refactoring
- Database modules are moved to separate files
- The FormatEvents trait and FormatEvent structure have been added to the Event module
- The FormatTasks trait has been added to the Task module
