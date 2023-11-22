use clap::Args;
use std::error::Error;

#[derive(Debug, Args)]
pub struct SumArgs {
    #[arg(required = true)]
    name: String,
}

pub fn cmd(sum_args: SumArgs) -> Result<(), Box<dyn Error>> {
    println!("Sum {}", &sum_args.name);

    Ok(())
}
