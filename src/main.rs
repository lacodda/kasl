use crate::commands::Cli;
use libs::update::Update;
use std::error::Error;

mod api;
mod commands;
mod db;
mod libs;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    Update::show_msg().await;
    Cli::menu().await
}
