//! Core library modules for the kasl application.
//!
//! This module serves as the main entry point for all kasl library components,
//! providing a centralized access point to the application's core functionality.
//! The library is organized into specialized modules that handle different
//! aspects of time tracking, task management, and productivity monitoring.
//!
//! ## Module Overview
//!
//! ### Core Infrastructure
//! - [`config`] - Configuration management and interactive setup
//! - [`data_storage`] - Cross-platform data directory management
//! - [`messages`] - Internationalized messaging and localization system
//!
//! ### Activity Monitoring
//! - [`monitor`] - Real-time activity tracking and pause detection
//! - [`daemon`] - Background process management and lifecycle
//! - [`pause`] - Work break analysis and management
//!
//! ### Data Management
//! - [`task`] - Task creation, editing, and lifecycle management
//! - [`report`] - Work interval calculation and productivity analysis
//! - [`summary`] - Monthly and daily work hour aggregation
//!
//! ### User Interface & Export
//! - [`view`] - Console table rendering and data visualization
//! - [`export`] - Data export to CSV, JSON, and Excel formats
//! - [`formatter`] - Time duration and data formatting utilities
//!
//! ### System Integration
//! - [`autostart`] - System boot integration and startup management
//! - [`update`] - Application self-update and version management
//! - [`secret`] - Secure credential storage with encryption
//!
//! ## Architecture
//!
//! The library follows a modular architecture where each component has
//! clearly defined responsibilities:
//!
//! ```text
//! ┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
//! │   User Input    │    │   System Events  │    │  External APIs  │
//! │   (CLI/GUI)     │    │   (Mouse/Keys)   │    │ (Jira/GitLab)   │
//! └─────────┬───────┘    └─────────┬────────┘    └─────────┬───────┘
//!           │                      │                       │
//!           ▼                      ▼                       ▼
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                    Core Library (libs)                         │
//! │  ┌───────────┐  ┌────────────┐  ┌──────────┐  ┌──────────────┐ │
//! │  │   task    │  │  monitor   │  │  config  │  │   messages   │ │
//! │  └───────────┘  └────────────┘  └──────────┘  └──────────────┘ │
//! │  ┌───────────┐  ┌────────────┐  ┌──────────┐  ┌──────────────┐ │
//! │  │  report   │  │   pause    │  │   view   │  │    export    │ │
//! │  └───────────┘  └────────────┘  └──────────┘  └──────────────┘ │
//! └─────────────────────────────────────────────────────────────────┘
//!           │                      │                       │
//!           ▼                      ▼                       ▼
//! ┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
//! │   Database      │    │   File System    │    │   Console       │
//! │   (SQLite)      │    │   (Config/Logs)  │    │   (Output)      │
//! └─────────────────┘    └──────────────────┘    └─────────────────┘
//! ```
//!
//! ## Usage Examples
//!
//! ### Task Management
//! ```rust,no_run
//! use kasl::libs::task::Task;
//! use kasl::db::tasks::Tasks;
//!
//! // Create and save a new task
//! let task = Task::new("Implement feature", "Add user authentication", Some(75));
//! let mut tasks_db = Tasks::new()?;
//! tasks_db.insert(&task)?;
//! ```
//!
//! ### Activity Monitoring
//! ```rust,no_run
//! use kasl::libs::monitor::Monitor;
//! use kasl::libs::config::MonitorConfig;
//!
//! // Start activity monitoring
//! let config = MonitorConfig::default();
//! let monitor = Monitor::new(config)?;
//! monitor.run().await?;
//! ```
//!
//! ### Report Generation
//! ```rust,no_run
//! use kasl::libs::report;
//! use chrono::Local;
//!
//! // Generate daily work report
//! let date = Local::now().date_naive();
//! let report = report::generate_daily(date)?;
//! ```
//!
//! ## Error Handling
//!
//! All modules use the [`anyhow`] crate for error handling, providing
//! rich error context and easy error propagation. Errors are typically
//! logged and displayed to users through the messaging system.
//!
//! ## Thread Safety
//!
//! Components that require concurrent access use appropriate synchronization
//! primitives. The monitor module uses `Arc<Mutex<T>>` for shared state,
//! while database operations are designed to be safe across threads.

pub mod autostart;
pub mod config;
pub mod daemon;
pub mod data_storage;
pub mod export;
pub mod formatter;
pub mod messages;
pub mod monitor;
pub mod pause;
pub mod report;
pub mod secret;
pub mod summary;
pub mod task;
pub mod update;
pub mod view;
