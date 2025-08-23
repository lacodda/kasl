//! Core library modules for the kasl application.
//!
//! Serves as the main entry point for all kasl library components, providing
//! a centralized access point to the application's core functionality.
//!
//! ## Features
//!
//! - **Core Infrastructure**: Configuration, data storage, messaging
//! - **Activity Monitoring**: Real-time tracking, daemon management, pause analysis
//! - **Productivity Analysis**: Centralized calculation engine with comprehensive metrics
//! - **Data Management**: Task lifecycle, reporting, summaries
//! - **User Interface**: Console rendering, data export, formatting
//! - **System Integration**: Autostart, updates, secure storage
//!
//! ## Usage
//!
//! ```rust,no_run
//! use kasl::libs::task::Task;
//! use kasl::db::tasks::Tasks;
//!
//! let task = Task::new("Implement feature", "Add user authentication", Some(75));
//! let mut tasks_db = Tasks::new()?;
//! tasks_db.insert(&task)?;
//! ```

pub mod autostart;
pub mod config;
pub mod daemon;
pub mod data_storage;
pub mod export;
pub mod formatter;
pub mod messages;
pub mod monitor;
pub mod pause;
pub mod productivity;
pub mod report;
pub mod secret;
pub mod summary;
pub mod task;
pub mod update;
pub mod view;
