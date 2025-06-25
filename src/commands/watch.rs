use crate::libs::config::Config;
use crate::libs::monitor::Monitor;
use std::error::Error;

// Runs the watch command to monitor user activity and record pauses asynchronously.
pub async fn cmd() -> Result<(), Box<dyn Error>> {
    let config = Config::read()?;
    let monitor_config = config.monitor.unwrap_or_default();
    let mut monitor = Monitor::new(monitor_config)?;
    monitor.run().await?;
    Ok(())
}
