use clap::Args;
use std::error::Error;

#[derive(Debug, Args)]
pub struct SyncArgs {
    #[arg(required = true)]
    name: String,
}

pub fn cmd(sync_args: SyncArgs) -> Result<(), Box<dyn Error>> {
    println!("Sync {}", &sync_args.name);

    Ok(())
}
