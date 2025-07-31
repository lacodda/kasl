use crate::{
    db::{pauses::Pauses, workdays::Workdays},
    libs::{formatter::format_duration, messages::Message},
    msg_bail_anyhow, msg_error, msg_info, msg_print, msg_success,
};
use anyhow::Result;
use chrono::{Duration, Local, NaiveDate, NaiveDateTime, NaiveTime};
use clap::{Args, ValueEnum};
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Select};

#[derive(Debug, Clone, ValueEnum)]
enum AdjustmentMode {
    /// Remove time from the start of the workday
    Start,
    /// Remove time from the end of the workday
    End,
    /// Add a pause in the middle of the workday
    Pause,
}

#[derive(Debug, Args)]
pub struct AdjustArgs {
    /// Date to adjust (YYYY-MM-DD or 'today')
    #[arg(long, short, default_value = "today")]
    date: String,

    /// Minutes to subtract or pause duration
    #[arg(long, short)]
    minutes: Option<u64>,

    /// Adjustment mode
    #[arg(long, value_enum)]
    mode: Option<AdjustmentMode>,

    /// Skip confirmation prompt
    #[arg(long)]
    force: bool,
}

pub async fn cmd(args: AdjustArgs) -> Result<()> {
    let date = parse_date(&args.date)?;
    let mut workdays_db = Workdays::new()?;

    // Get current workday
    let workday = match workdays_db.fetch(date)? {
        Some(wd) => wd,
        None => {
            msg_error!(Message::WorkdayNotFoundForDate(date.format("%B %-d, %Y").to_string()));
            return Ok(());
        }
    };

    // Get adjustment parameters interactively if not provided
    let mode = args.mode.unwrap_or_else(|| {
        let options = vec!["Start - Remove time from start", "End - Remove time from end", "Pause - Add a pause"];
        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt(Message::SelectAdjustmentMode.to_string())
            .items(&options)
            .default(0)
            .interact()
            .unwrap();

        match selection {
            0 => AdjustmentMode::Start,
            1 => AdjustmentMode::End,
            _ => AdjustmentMode::Pause,
        }
    });

    let minutes = args.minutes.unwrap_or_else(|| {
        Input::with_theme(&ColorfulTheme::default())
            .with_prompt(Message::PromptAdjustmentMinutes.to_string())
            .validate_with(|input: &u64| -> Result<(), &str> {
                if *input > 0 && *input <= 480 {
                    // Max 8 hours
                    Ok(())
                } else {
                    Err("Minutes must be between 1 and 480")
                }
            })
            .interact_text()
            .unwrap()
    });

    // Calculate and show preview
    let preview = calculate_preview(&workday, &mode, minutes)?;
    display_preview(&workday, &preview, &mode, minutes)?;

    // Confirm changes
    if !args.force {
        let confirmed = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(Message::ConfirmTimeAdjustment.to_string())
            .default(false)
            .interact()?;

        if !confirmed {
            msg_info!(Message::OperationCancelled);
            return Ok(());
        }
    }

    // Apply changes
    apply_adjustment(&mut workdays_db, &workday, &mode, minutes, date)?;

    msg_success!(Message::TimeAdjustmentApplied);
    Ok(())
}

#[derive(Debug)]
struct AdjustmentPreview {
    new_start: NaiveDateTime,
    new_end: Option<NaiveDateTime>,
    new_duration: Duration,
    pause_to_add: Option<(NaiveDateTime, NaiveDateTime)>,
}

fn calculate_preview(workday: &crate::db::workdays::Workday, mode: &AdjustmentMode, minutes: u64) -> Result<AdjustmentPreview> {
    let duration_to_adjust = Duration::minutes(minutes as i64);
    let end_time = workday.end.unwrap_or_else(|| Local::now().naive_local());
    let current_duration = end_time - workday.start;

    match mode {
        AdjustmentMode::Start => {
            let new_start = workday.start + duration_to_adjust;
            if new_start >= end_time {
                msg_bail_anyhow!(Message::InvalidAdjustmentTooMuchTime);
            }
            Ok(AdjustmentPreview {
                new_start,
                new_end: workday.end,
                new_duration: current_duration - duration_to_adjust,
                pause_to_add: None,
            })
        }
        AdjustmentMode::End => {
            let new_end = end_time - duration_to_adjust;
            if new_end <= workday.start {
                msg_bail_anyhow!(Message::InvalidAdjustmentTooMuchTime);
            }
            Ok(AdjustmentPreview {
                new_start: workday.start,
                new_end: Some(new_end),
                new_duration: current_duration - duration_to_adjust,
                pause_to_add: None,
            })
        }
        AdjustmentMode::Pause => {
            // For pause mode, ask when to insert the pause
            let pause_start_time = Input::<String>::with_theme(&ColorfulTheme::default())
                .with_prompt(Message::PromptPauseStartTime.to_string())
                .default("12:00".to_string())
                .validate_with(|input: &String| -> Result<(), &str> {
                    NaiveTime::parse_from_str(input, "%H:%M")
                        .map(|_| ())
                        .map_err(|_| "Invalid time format. Use HH:MM")
                })
                .interact_text()?;

            let pause_time = NaiveTime::parse_from_str(&pause_start_time, "%H:%M")?;
            let pause_start = workday.start.date().and_time(pause_time);
            let pause_end = pause_start + duration_to_adjust;

            // Validate pause is within workday
            if pause_start < workday.start || pause_end > end_time {
                msg_bail_anyhow!(Message::InvalidPauseOutsideWorkday);
            }

            Ok(AdjustmentPreview {
                new_start: workday.start,
                new_end: workday.end,
                new_duration: current_duration - duration_to_adjust,
                pause_to_add: Some((pause_start, pause_end)),
            })
        }
    }
}

fn display_preview(workday: &crate::db::workdays::Workday, preview: &AdjustmentPreview, _mode: &AdjustmentMode, minutes: u64) -> Result<()> {
    let end_time = workday.end.unwrap_or_else(|| Local::now().naive_local());
    let current_duration = end_time - workday.start;

    msg_print!(Message::AdjustmentPreview, true);

    println!("Current workday:");
    println!("  Start: {}", workday.start.format("%H:%M"));
    println!("  End:   {}", end_time.format("%H:%M"));
    println!("  Total: {}", format_duration(&current_duration));

    println!("\nAfter adjustment:");
    println!("  Start: {}", preview.new_start.format("%H:%M"));
    println!("  End:   {}", preview.new_end.unwrap_or(end_time).format("%H:%M"));
    println!(
        "  Total: {} (-{})",
        format_duration(&preview.new_duration),
        format_duration(&Duration::minutes(minutes as i64))
    );

    if let Some((pause_start, pause_end)) = &preview.pause_to_add {
        println!("\nNew pause:");
        println!("  {} - {}", pause_start.format("%H:%M"), pause_end.format("%H:%M"));
    }

    Ok(())
}

fn apply_adjustment(workdays_db: &mut Workdays, workday: &crate::db::workdays::Workday, mode: &AdjustmentMode, minutes: u64, date: NaiveDate) -> Result<()> {
    let duration_to_adjust = Duration::minutes(minutes as i64);

    match mode {
        AdjustmentMode::Start => {
            let new_start = workday.start + duration_to_adjust;
            workdays_db.update_start(date, new_start)?;
        }
        AdjustmentMode::End => {
            let end_time = workday.end.unwrap_or_else(|| Local::now().naive_local());
            let new_end = end_time - duration_to_adjust;
            workdays_db.update_end(date, Some(new_end))?;
        }
        AdjustmentMode::Pause => {
            // Calculate pause times (this duplicates preview logic, but ensures consistency)
            let pause_start_time = Input::<String>::with_theme(&ColorfulTheme::default())
                .with_prompt(Message::PromptPauseStartTime.to_string())
                .default("12:00".to_string())
                .interact_text()?;

            let pause_time = NaiveTime::parse_from_str(&pause_start_time, "%H:%M")?;
            let pause_start = workday.start.date().and_time(pause_time);
            let pause_end = pause_start + duration_to_adjust;

            // Add the pause
            let pauses_db = Pauses::new()?;
            pauses_db.insert_start_with_time(pause_start)?;

            // We need to get the ID of the inserted pause and update its end time
            let pauses = pauses_db.fetch(date, 0)?;
            if let Some(last_pause) = pauses.iter().rev().find(|p| p.start == pause_start) {
                // Update pause end time directly in the database
                let conn_guard = pauses_db.conn.lock();
                conn_guard.execute(
                    "UPDATE pauses SET end = ?1, duration = ?2 WHERE id = ?3",
                    rusqlite::params![
                        pause_end.format("%Y-%m-%d %H:%M:%S").to_string(),
                        duration_to_adjust.num_seconds(),
                        last_pause.id
                    ],
                )?;
            }
        }
    }

    Ok(())
}

fn parse_date(date_str: &str) -> Result<NaiveDate> {
    if date_str.to_lowercase() == "today" {
        Ok(Local::now().date_naive())
    } else {
        Ok(NaiveDate::parse_from_str(date_str, "%Y-%m-%d")?)
    }
}
