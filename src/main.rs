use kasl::commands::Cli;
use kasl::libs::update::Update;
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    Update::show_msg().await;
    Cli::menu().await
}
