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
use crate::libs::config::Config;
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

/// Calculates the break duration needed to reach target productivity.
///
/// This function determines how many minutes of manual breaks need to be added
/// to achieve a specific productivity threshold. This is used for generating
/// break recommendations when productivity falls below acceptable levels.
///
/// ## Calculation Logic
///
/// The function works backwards from the target productivity:
/// 1. Calculate current net work time and gross time
/// 2. Determine required available work time for target productivity
/// 3. Calculate needed break duration to achieve that available work time
/// 4. Account for existing manual breaks in the calculation
///
/// ## Productivity Improvement Strategy
///
/// By adding manual breaks:
/// - **Gross time**: Remains the same (workday boundaries unchanged)
/// - **Available work time**: Decreases (manual breaks excluded)
/// - **Net work time**: Decreases slightly (existing pauses unchanged)
/// - **Productivity ratio**: Improves (net/available increases)
///
/// # Arguments
///
/// * `workday` - The workday record with current timing
/// * `pauses` - Existing automatic pauses
/// * `existing_breaks` - Manual breaks already added
/// * `target_productivity` - Desired productivity percentage (0.0-100.0)
///
/// # Returns
///
/// Returns the number of minutes of breaks needed to reach the target,
/// or 0 if the target is already achieved or impossible to reach.
///
/// # Examples
///
/// ```rust
/// let needed_minutes = calculate_needed_break_duration(
///     &workday, &pauses, &existing_breaks, 75.0
/// );
/// 
/// if needed_minutes > 0 {
///     println!("Add {} minutes of breaks to reach 75% productivity", needed_minutes);
/// }
/// ```
pub fn calculate_needed_break_duration(
    workday: &Workday,
    pauses: &[Pause], 
    existing_breaks: &[Break],
    target_productivity: f64,
) -> u64 {
    let end_time = workday.end.unwrap_or_else(|| chrono::Local::now().naive_local());
    let gross_duration = end_time - workday.start;
    
    // Calculate current state
    let pause_duration: Duration = pauses
        .iter()
        .filter_map(|p| p.duration)
        .sum();
    
    let existing_break_duration: Duration = existing_breaks
        .iter()
        .map(|b| b.duration)
        .sum();
    
    let net_work_time = gross_duration - pause_duration - existing_break_duration;
    
    // Validate input parameters
    if target_productivity <= 0.0 || target_productivity > 100.0 {
        return 0; // Invalid target productivity
    }
    
    if net_work_time.num_seconds() <= 0 {
        return 0; // No net work time available
    }
    
    // Calculate required available work time for target productivity
    // target_productivity = (net_work_time / available_work_time) * 100
    // available_work_time = net_work_time * 100 / target_productivity
    let required_available_time_seconds = 
        (net_work_time.num_seconds() as f64 * 100.0 / target_productivity) as i64;
    let required_available_time = Duration::seconds(required_available_time_seconds);
    
    // Calculate total break duration needed
    // available_work_time = gross_duration - total_break_duration
    // total_break_duration = gross_duration - required_available_time
    let total_needed_break_duration = gross_duration - required_available_time;
    
    // Calculate additional break duration needed beyond existing breaks
    let additional_break_duration = total_needed_break_duration - existing_break_duration;
    
    // Return additional minutes needed, ensuring non-negative result
    if additional_break_duration.num_minutes() > 0 {
        additional_break_duration.num_minutes() as u64
    } else {
        0
    }
}

/// Check if productivity suggestions should be made based on workday progress.
///
/// This function determines whether enough of the workday has passed to make
/// meaningful productivity recommendations. It prevents premature suggestions
/// when the workday has just started.
///
/// # Arguments
///
/// * `workday` - The current workday record
/// * `workday_hours` - Expected total workday duration in hours
/// * `min_fraction` - Minimum fraction of workday that must pass before suggestions
///
/// # Returns
///
/// `true` if suggestions should be made, `false` otherwise
///
/// # Examples
///
/// ```rust
/// if should_suggest_productivity_improvements(&workday, 8.0, 0.25) {
///     // Make productivity recommendations
/// }
/// ```
pub fn should_suggest_productivity_improvements(
    workday: &Workday,
    workday_hours: f64,
    min_fraction: f64,
) -> bool {
    let now = chrono::Local::now().naive_local();
    let elapsed = now - workday.start;
    let expected_duration = Duration::seconds((workday_hours * 3600.0) as i64);
    let min_duration = Duration::seconds((expected_duration.num_seconds() as f64 * min_fraction) as i64);
    
    elapsed >= min_duration
}

/// Check if productivity recommendations should be shown and calculate needed break duration.
///
/// This function combines productivity checking with break duration calculation to provide
/// a complete recommendation system. It checks if suggestions should be made and calculates
/// the break duration needed to reach the target productivity threshold.
///
/// # Arguments
///
/// * `workday` - The current workday record
/// * `pauses` - Existing automatic pauses  
/// * `breaks` - Manual breaks already added
/// * `config` - Configuration containing productivity thresholds
///
/// # Returns
///
/// Returns `Some(needed_minutes)` if recommendations should be shown,
/// `None` if productivity is acceptable or recommendations shouldn't be made yet.
///
/// # Examples
///
/// ```rust
/// if let Some(needed_minutes) = check_productivity_recommendations(&workday, &pauses, &breaks, &config) {
///     println!("Consider adding {} minutes of breaks", needed_minutes);
/// }
/// ```
pub fn check_productivity_recommendations(
    workday: &Workday,
    pauses: &[Pause],
    breaks: &[Break],
    config: &Config,
) -> Option<u64> {
    // Get productivity configuration with defaults
    let productivity_config = config.productivity.as_ref().cloned().unwrap_or_default();
    
    // Check if enough of the workday has passed to make suggestions
    if !should_suggest_productivity_improvements(
        workday,
        productivity_config.workday_hours,
        productivity_config.min_workday_fraction_before_suggest,
    ) {
        return None; // Too early to suggest improvements
    }
    
    // Calculate current productivity including manual breaks
    let current_productivity = calculate_productivity(workday, pauses, breaks);
    
    // Check if productivity is below the minimum threshold
    if current_productivity >= productivity_config.min_productivity_threshold {
        return None; // Productivity is acceptable
    }
    
    // Calculate needed break duration to reach minimum productivity
    let needed_minutes = calculate_needed_break_duration(
        workday,
        pauses,
        breaks,
        productivity_config.min_productivity_threshold,
    );
    
    // Only show recommendations if a meaningful break can help
    if needed_minutes >= productivity_config.min_break_duration
        && needed_minutes <= productivity_config.max_break_duration
    {
        Some(needed_minutes)
    } else {
        None
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
    let break_duration: Duration = breaks_in_intervals
        .iter()
        .map(|b| b.duration)
        .sum();
    
    // For interval-based calculation, subtract both pauses and breaks from filtered duration
    let net_work_time = filtered_work_duration
        .checked_sub(&pause_duration)
        .unwrap_or_else(|| Duration::zero())
        .checked_sub(&break_duration)
        .unwrap_or_else(|| Duration::zero());
    
    // Available time is the filtered duration minus breaks (pauses are interruptions)
    let available_work_time = filtered_work_duration.checked_sub(&break_duration)
        .unwrap_or_else(|| Duration::zero());
    
    if available_work_time.num_seconds() > 0 {
        let productivity = (net_work_time.num_seconds() as f64 / available_work_time.num_seconds() as f64) * 100.0;
        productivity.max(0.0).min(100.0) // Clamp between 0-100%
    } else {
        0.0
    }
}

