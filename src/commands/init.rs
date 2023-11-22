use clap::Args;
use std::error::Error;

#[derive(Debug, Args)]
pub struct InitArgs {
    #[arg(required = true)]
    name: String,
}

pub fn cmd(init_args: InitArgs) -> Result<(), Box<dyn Error>> {
    println!("Init {}", &init_args.name);

    Ok(())
}
