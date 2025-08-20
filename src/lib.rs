//! # Kasl - Key Activity Synchronization and Logging
//!
//! A command-line utility for tracking work activities, managing tasks,
//! and generating productivity reports.
//!
//! ## Features
//!
//! - **Activity Monitoring**: Automatic detection of work sessions and breaks
//! - **Task Management**: Create, update, and track task completion
//! - **Report Generation**: Daily and monthly productivity reports
//! - **External Integrations**: Sync with GitLab commits and Jira issues
//! - **Data Export**: Export data to CSV, JSON, and Excel formats
//! - **Template System**: Reusable task templates
//! - **Tag System**: Organize tasks with custom tags
//!
//! ## Usage
//!
//! ```rust,no_run
//! use kasl::commands::Cli;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     Cli::menu().await
//! }
//! ```

pub mod api;
pub mod commands;
pub mod db;
pub mod libs;
