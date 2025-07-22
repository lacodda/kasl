/// Convenience macros for common message operations

/// Print a message
#[macro_export]
macro_rules! msg_print {
    ($msg:expr) => {
        println!("{}", $msg)
    };
    ($msg:expr, true) => {
        println!("\n{}\n", $msg)
    };
}

/// Print a success message with ✅ prefix
#[macro_export]
macro_rules! msg_success {
    ($msg:expr) => {
        println!("✅ {}", $msg)
    };
    ($msg:expr, true) => {
        println!("\n✅ {}\n", $msg)
    };
}

/// Print an error message with ❌ prefix
#[macro_export]
macro_rules! msg_error {
    ($msg:expr) => {
        eprintln!("❌ {}", $msg)
    };
    ($msg:expr, true) => {
        println!("\n❌ {}\n", $msg)
    };
}

/// Print a warning message with ⚠️ prefix
#[macro_export]
macro_rules! msg_warning {
    ($msg:expr) => {
        println!("⚠️ {}", $msg)
    };
    ($msg:expr, true) => {
        println!("\n⚠️ {}\n", $msg)
    };
}

/// Print an info message with ℹ️ prefix
#[macro_export]
macro_rules! msg_info {
    ($msg:expr) => {
        println!("ℹ️ {}", $msg)
    };
    ($msg:expr, true) => {
        println!("\nℹ️ {}\n", $msg)
    };
}

/// Create an anyhow error from a message
#[macro_export]
macro_rules! msg_error_anyhow {
    ($msg:expr) => {
        anyhow::anyhow!("❌ {}", $msg)
    };
}

/// Bail with a message error
#[macro_export]
macro_rules! msg_bail_anyhow {
    ($msg:expr) => {
        anyhow::bail!("❌ {}", $msg)
    };
}
