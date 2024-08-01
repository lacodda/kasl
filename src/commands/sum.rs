use crate::{
    api::si::Si,
    db::events::{Events, SelectRequest},
    libs::{
        config::Config,
        event::{EventGroup, EventGroupDuration, EventGroupTotalDuration},
        view::View,
    },
};
use chrono::{Duration, Local, NaiveDate};
use clap::Args;
use std::{collections::HashSet, error::Error};

#[derive(Debug, Args)]
pub struct SumArgs {
    #[arg(long, help = "Send report")]
    send: bool,
}

pub async fn cmd(_sum_args: SumArgs) -> Result<(), Box<dyn Error>> {
    let now = Local::now();
    println!("\nWorking hours for {}", now.format("%B, %Y"));
    let mut rest_dates: HashSet<NaiveDate> = HashSet::new();
    let duration: Duration = Duration::hours(8);
    match Config::read() {
        Ok(config) => match config.si {
            Some(si_config) => match Si::new(&si_config).rest_dates(now.date_naive()).await {
                Ok(dates) => {
                    rest_dates = dates;
                }
                Err(e) => eprintln!("Error requesting rest dates: {}", e),
            },
            None => eprintln!("Failed to read SiServer config"),
        },
        Err(e) => eprintln!("Failed to read config: {}", e),
    }

    let event_summary = Events::new()?
        .fetch(SelectRequest::Monthly, now.date_naive())?
        .group_events()
        .calc()
        .add_rest_dates(rest_dates, duration)
        .total_duration()
        .format();

    View::sum(&event_summary)?;

    Ok(())
}
