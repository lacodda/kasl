use clap::Args;
use std::error::Error;

#[derive(Debug, Args)]
pub struct TimeArgs {
    #[arg(required = true)]
    name: String,
}

pub fn cmd(time_args: TimeArgs) -> Result<(), Box<dyn Error>> {
    println!("Time {}", &time_args.name);

    Ok(())
}
