use crate::libs::monitor::{Monitor, MonitorConfig};
use std::error::Error;

// Runs the watch command to monitor user activity and record breaks asynchronously.
pub async fn cmd() -> Result<(), Box<dyn Error>> {
    let config = MonitorConfig {
        breaks_enabled: true,
        break_threshold: 60, // 60 seconds
        poll_interval: 500,  // 500 milliseconds
    };

    let monitor = Monitor::new(config)?;
    monitor.run().await?;
    Ok(())
}
