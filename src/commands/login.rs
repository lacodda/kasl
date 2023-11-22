use clap::Args;
use std::error::Error;

#[derive(Debug, Args)]
pub struct LoginArgs {
    #[arg(required = true)]
    name: String,
}

pub fn cmd(login_args: LoginArgs) -> Result<(), Box<dyn Error>> {
    println!("Login {}", &login_args.name);

    Ok(())
}
