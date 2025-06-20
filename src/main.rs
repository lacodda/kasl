use crate::commands::Cli;
use libs::monitor::{Monitor, MonitorConfig};
use libs::update::Update;
use std::error::Error;

mod api;
mod commands;
mod db;
mod libs;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Temporary configuration loading. This will be replaced with a more robust solution later.
    let config = MonitorConfig {
        breaks_enabled: true, // Enables break monitoring.
        break_threshold: 10,  // Sets the inactivity threshold for breaks to 60 seconds.
        poll_interval: 500,   // Sets the activity poll interval to 500 milliseconds.
    };

    // Initializes the Monitor with the given configuration
    let monitor = Monitor::new(config)?;
    tokio::spawn(async move {
        monitor.run().await.unwrap();
    });

    Update::show_msg().await;
    let _ = Cli::menu().await;
    Ok(())
}
