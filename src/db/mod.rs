//! SQLite persistence layer: one module per entity, migrations on open.
//!
//! ```rust,no_run
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! use kasl::db::{db::Db, tasks::Tasks, workdays::Workdays};
//! use kasl::libs::task::Task;
//!
//! let db = Db::new()?;
//! let mut tasks = Tasks::new()?;
//! let task = Task::new("Review code", "Check PR #123", Some(75));
//! tasks.insert(&task)?;
//! # Ok(())
//! # }
//! ```
//!
//! ```rust,no_run
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! use kasl::db::{tags::{Tags, Tag}, templates::{Templates, TaskTemplate}};
//!
//! let mut tags = Tags::new()?;
//! let tag_id = tags.create(&Tag::new("urgent".to_string(), Some("red".to_string())))?;
//!
//! let mut templates = Templates::new()?;
//! let template = TaskTemplate::new(
//!     "daily-standup".to_string(),
//!     "Attend daily standup meeting".to_string(),
//!     "Team sync and planning".to_string(),
//!     100
//! );
//! templates.create(&template)?;
//! # Ok(())
//! # }
//! ```

/// Connection handling and schema bootstrap.
#[allow(clippy::module_inception)] // kasl::db::db::Db is the established public path
pub mod db;

/// Versioned schema migrations.
pub mod migrations;

/// Local inbox of assigned open Jira issues.
pub mod jira_inbox;

/// Catalog of Jira status id → name pairs synced from issues.
pub mod jira_statuses;

/// Pause records detected by the monitor or entered by hand.
pub mod pauses;

/// Tags and their task associations.
pub mod tags;

/// Task CRUD and filtered queries.
pub mod tasks;

/// Reusable task templates.
pub mod templates;

/// Workday start/end records.
pub mod workdays;
