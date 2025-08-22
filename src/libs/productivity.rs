//! Productivity calculation utilities for work time analysis.
//!
//! This module provides centralized productivity calculations that properly handle
//! different types of work interruptions (pauses vs. breaks) to give accurate
//! productivity metrics.
//!
//! ## Key Concepts
//!
//! - **Short Pauses**: Brief interruptions that are not recorded in the database (< min_pause_duration)
//! - **Long Pauses**: Extended interruptions that are recorded as pause records (>= min_pause_duration)  
//! - **Manual Breaks**: User-defined break periods that are excluded from productivity calculations
//!
//! ## Productivity Formula
//!
//! ```text
//! Productivity = (Net Work Time / Available Work Time) * 100
//!
//! Where:
//! - Net Work Time = Total Time - Long Pauses - Manual Breaks
//! - Available Work Time = Total Time - Manual Breaks
//! ```

use crate::db::breaks::Break;
use crate::libs::pause::Pause;
use crate::db::workdays::Workday;
use chrono::Duration;

/// Calculate productivity percentage based on workday data.
///
/// This function provides the standard productivity calculation that considers
/// recorded pauses (long pauses) as productivity losses, while treating manual
/// breaks as excluded time periods.
///
/// # Arguments
///
/// * `workday` - The workday record containing start/end times
/// * `pauses` - Recorded pause periods (long pauses >= min_pause_duration)
/// * `breaks` - Manual break periods defined by the user
///
/// # Returns
///
/// Productivity percentage (0.0 to 100.0)
///
/// # Examples
///
/// ```rust
/// use kasl::libs::productivity::calculate_productivity;
///
/// let productivity = calculate_productivity(&workday, &pauses, &breaks);
/// println!("Productivity: {:.1}%", productivity);
/// ```
pub fn calculate_productivity(
    workday: &Workday,
    pauses: &[Pause],
    breaks: &[Break],
) -> f64 {
    let end_time = workday.end.unwrap_or_else(|| chrono::Local::now().naive_local());
    let gross_duration = end_time - workday.start;
    
    // Calculate total recorded pause time (these are "long pauses")
    let pause_duration: Duration = pauses
        .iter()
        .filter_map(|p| p.duration)
        .sum();
    
    // Calculate total manual break time
    let break_duration: Duration = breaks
        .iter()
        .map(|b| b.duration)
        .sum();
    
    // Net work time = gross time - recorded pauses - manual breaks
    let net_work_time = gross_duration - pause_duration - break_duration;
    
    // Available work time = gross time - manual breaks
    // (recorded pauses are considered interruptions, not excluded time)
    let available_work_time = gross_duration - break_duration;
    
    if available_work_time.num_seconds() > 0 {
        let productivity = (net_work_time.num_seconds() as f64 / available_work_time.num_seconds() as f64) * 100.0;
        productivity.max(0.0).min(100.0) // Clamp between 0-100%
    } else {
        0.0
    }
}

/// Calculate productivity for filtered work intervals.
///
/// This is used when displaying productivity for specific time periods or
/// filtered work sessions. It calculates productivity based on the filtered
/// work time and corresponding pauses within those intervals.
///
/// # Arguments
///
/// * `filtered_work_duration` - Total duration of filtered work intervals
/// * `pauses_in_intervals` - Pauses that occurred within the filtered intervals
/// * `breaks_in_intervals` - Manual breaks that occurred within the filtered intervals
///
/// # Returns
///
/// Productivity percentage (0.0 to 100.0)
///
/// # Examples
///
/// ```rust
/// use kasl::libs::productivity::calculate_productivity_for_intervals;
/// use chrono::Duration;
///
/// let work_time = Duration::hours(6);
/// let productivity = calculate_productivity_for_intervals(&work_time, &[], &[]);
/// ```
pub fn calculate_productivity_for_intervals(
    filtered_work_duration: &Duration,
    pauses_in_intervals: &[Pause],
    breaks_in_intervals: &[Break],
) -> f64 {
    // Calculate pause time within the intervals
    let pause_duration: Duration = pauses_in_intervals
        .iter()
        .filter_map(|p| p.duration)
        .sum();
    
    // Calculate break time within the intervals
    let _break_duration: Duration = breaks_in_intervals
        .iter()
        .map(|b| b.duration)
        .sum();
    
    // For interval-based calculation, we assume the filtered_work_duration
    // already excludes breaks, so we only subtract pauses
    let net_work_time = filtered_work_duration.checked_sub(&pause_duration)
        .unwrap_or_else(|| Duration::zero());
    
    // Available time is the filtered duration (breaks should already be excluded)
    let available_work_time = *filtered_work_duration;
    
    if available_work_time.num_seconds() > 0 {
        let productivity = (net_work_time.num_seconds() as f64 / available_work_time.num_seconds() as f64) * 100.0;
        productivity.max(0.0).min(100.0) // Clamp between 0-100%
    } else {
        0.0
    }
}

