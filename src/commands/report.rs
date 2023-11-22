use clap::Args;
use std::error::Error;

#[derive(Debug, Args)]
pub struct ReportArgs {
    #[arg(required = true)]
    name: String,
}

pub fn cmd(report_args: ReportArgs) -> Result<(), Box<dyn Error>> {
    println!("Report {}", &report_args.name);

    Ok(())
}
