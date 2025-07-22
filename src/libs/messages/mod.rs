pub mod display;
pub mod macros;
pub mod types;

pub use types::Message;

// Convenience functions for common message patterns
pub fn success(msg: Message) -> String {
    format!("✅ {}", msg)
}

pub fn error(msg: Message) -> String {
    format!("❌ {}", msg)
}

pub fn warning(msg: Message) -> String {
    format!("⚠️  {}", msg)
}

pub fn info(msg: Message) -> String {
    format!("ℹ️  {}", msg)
}

pub fn wrap_msg(msg: Message) -> String {
    format!("\n{}\n", msg)
}
