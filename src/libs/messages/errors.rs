// Database errors
pub const DB_CONNECTION_FAILED: &str = "Failed to connect to database";
pub const DB_QUERY_FAILED: &str = "Database query failed";
pub const DB_MIGRATION_FAILED: &str = "Database migration failed";

// Configuration errors
pub const CONFIG_FILE_NOT_FOUND: &str = "Configuration file not found";
pub const CONFIG_PARSE_ERROR: &str = "Failed to parse configuration";
pub const CONFIG_SAVE_ERROR: &str = "Failed to save configuration";

// API errors
pub const API_CONNECTION_FAILED: &str = "Failed to connect to API";
pub const API_AUTH_FAILED: &str = "API authentication failed";
pub const API_REQUEST_FAILED: &str = "API request failed";
pub const GITLAB_FETCH_FAILED: &str = "[kasl] Failed to get GitLab events";
pub const JIRA_FETCH_FAILED: &str = "[kasl] Failed to get Jira issues";

// Task errors
pub const TASK_NOT_FOUND: &str = "Task not found";
pub const TASK_CREATE_FAILED: &str = "Failed to create task";
pub const TASK_UPDATE_FAILED: &str = "Failed to update task";
pub const TASK_DELETE_FAILED: &str = "Failed to delete task";

// Workday errors
pub const WORKDAY_NOT_FOUND: &str = "No workday record found";
pub const WORKDAY_CREATE_FAILED: &str = "Failed to create workday";

// Monitor errors
pub const MONITOR_START_FAILED: &str = "Failed to start monitor";
pub const MONITOR_STOP_FAILED: &str = "Failed to stop monitor";

// File system errors
pub const FILE_NOT_FOUND: &str = "File not found";
pub const FILE_READ_ERROR: &str = "Failed to read file";
pub const FILE_WRITE_ERROR: &str = "Failed to write file";

// Session errors
pub const SESSION_EXPIRED: &str = "Session expired";
pub const INVALID_CREDENTIALS: &str = "Invalid credentials";

// Generic errors
pub const OPERATION_CANCELLED: &str = "Operation cancelled";
pub const INVALID_INPUT: &str = "Invalid input provided";
pub const PERMISSION_DENIED: &str = "Permission denied";
