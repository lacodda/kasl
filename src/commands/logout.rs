use clap::Args;
use std::error::Error;

#[derive(Debug, Args)]
pub struct LogoutArgs {
    #[arg(required = true)]
    name: String,
}

pub fn cmd(logout_args: LogoutArgs) -> Result<(), Box<dyn Error>> {
    println!("Logout {}", &logout_args.name);

    Ok(())
}
