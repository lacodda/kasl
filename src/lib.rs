//! # Kasl - Key Activity Synchronization and Logging
//!
//! A comprehensive command-line utility for tracking work activities, managing tasks,
//! and generating productivity reports. This library provides the core functionality
//! for monitoring user activity, managing work sessions, and integrating with
//! external services.
//!
//! ## Main Modules
//!
//! - [`api`] - External API integrations (GitLab, Jira, SiServer)
//! - [`commands`] - CLI command implementations
//! - [`db`] - Database operations and models
//! - [`libs`] - Core library utilities and helpers
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
//! ## Example
//!
//! ```rust,no_run
//! use kasl::commands::Cli;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     // Initialize and run the CLI
//!     Cli::menu().await
//! }
//! ```

pub mod api;
pub mod commands;
pub mod db;
pub mod libs;
