//! The hourly (SiServer-style) report grid: hour slots classified by
//! work/break overlap, tasks distributed across them, rows rendered.
//!
//! The rendering itself (Excel layout, fonts, template) stays in the
//! parent module; this one owns the slot math, which is what the tests
//! pin down.

#[cfg(test)]
use crate::libs::locale::Language;
use crate::libs::locale::Locale;
use crate::libs::pause::Pause;
use crate::libs::report::WorkInterval;
use crate::libs::task::Task;
use chrono::{Duration, NaiveDate, NaiveDateTime, Timelike};

/// A single rendered row of the hourly report (one hour of the workday).
pub(super) struct HourlyRow {
    /// Hour slot start in "HH:MM" format.
    pub(super) start: String,
    /// Hour slot end in "HH:MM" format.
    pub(super) end: String,
    /// Description of what happened during the hour ("Перерыв" for breaks).
    pub(super) description: String,
}

/// Aggregated data required to render an hourly daily report.
pub(super) struct HourlyReport {
    /// Report date.
    pub(super) date: NaiveDate,
    /// Localized weekday name.
    pub(super) weekday: String,
    /// Localized month name.
    pub(super) month: String,
    /// Whole worked hours, used for the "workday length" header cell.
    pub(super) day_hours: i64,
    /// Total net worked time formatted as "HH:MM".
    pub(super) worked: String,
    /// Hour-by-hour rows.
    pub(super) rows: Vec<HourlyRow>,
}

/// One hour-aligned slot of the workday with work/break classification flags.
#[derive(Debug, Clone)]
pub(super) struct HourSlot {
    /// Grid start of the hour (minutes/seconds zeroed).
    start: NaiveDateTime,
    /// Slot end (may be earlier than `start + 1h` on the last hour).
    end: NaiveDateTime,
    /// Whether the slot overlaps any work interval.
    has_work: bool,
    /// Whether the slot overlaps any break/pause.
    has_break: bool,
}

/// Truncates a timestamp down to the start of its hour (zeroing minutes/seconds).
fn floor_to_hour(dt: NaiveDateTime) -> NaiveDateTime {
    dt.with_minute(0)
        .and_then(|d| d.with_second(0))
        .and_then(|d| d.with_nanosecond(0))
        .unwrap_or(dt)
}

/// Returns `true` when `[a_start, a_end)` overlaps `[b_start, b_end)`.
fn ranges_overlap(a_start: NaiveDateTime, a_end: NaiveDateTime, b_start: NaiveDateTime, b_end: NaiveDateTime) -> bool {
    a_start < b_end && b_start < a_end
}

/// Builds hour-aligned slots covering `[work_start, work_end)` and classifies
/// each slot by overlap with work intervals and interruptions.
pub(super) fn classify_hour_slots(work_start: NaiveDateTime, work_end: NaiveDateTime, intervals: &[WorkInterval], interruptions: &[Pause]) -> Vec<HourSlot> {
    let mut slots = Vec::new();
    if work_end <= work_start {
        return slots;
    }

    let mut slot_start = floor_to_hour(work_start);
    while slot_start < work_end {
        let slot_grid_end = slot_start + Duration::hours(1);
        let slot_end = slot_grid_end.min(work_end);
        let window_start = slot_start.max(work_start);

        let has_work = intervals
            .iter()
            .any(|interval| ranges_overlap(window_start, slot_end, interval.start, interval.end));
        let has_break = interruptions.iter().any(|pause| {
            let Some(pause_end) = pause.end else {
                return false;
            };
            let start = pause.start.max(work_start);
            let end = pause_end.min(work_end);
            start < end && ranges_overlap(window_start, slot_end, start, end)
        });

        slots.push(HourSlot {
            start: slot_start,
            end: slot_end,
            has_work,
            has_break,
        });

        slot_start = slot_grid_end;
    }

    slots
}

/// Distributes task descriptions across hour slots (one primary task per work hour).
///
/// - Work hours receive task labels; pure break hours stay empty (`None`).
/// - When there are fewer tasks than work hours, each task occupies a contiguous
///   block of consecutive work hours.
/// - When there are more tasks than work hours, each work hour gets one task and
///   surplus tasks are appended (joined with `". "`) only to hours without a break.
///   If every work hour has a break, surplus tasks fall back onto work hours in order.
/// - With no tasks, work hours use the locale's generic work label.
pub(super) fn assign_tasks_to_hour_slots(tasks: &[Task], slots: &[HourSlot], locale: &Locale) -> Vec<Option<String>> {
    let mut texts: Vec<Option<String>> = vec![None; slots.len()];
    let work_indices: Vec<usize> = slots.iter().enumerate().filter(|(_, s)| s.has_work).map(|(i, _)| i).collect();

    if work_indices.is_empty() {
        return texts;
    }

    if tasks.is_empty() {
        for &idx in &work_indices {
            texts[idx] = Some(locale.work_generic.to_string());
        }
        return texts;
    }

    let num_work = work_indices.len();
    let num_tasks = tasks.len();

    if num_tasks <= num_work {
        // Contiguous blocks: A A A B B C …
        let base = num_work / num_tasks;
        let mut extra = num_work % num_tasks;
        let mut cursor = 0usize;

        for task in tasks {
            let count = base + if extra > 0 { 1 } else { 0 };
            extra = extra.saturating_sub(1);
            let text = locale.work_text(&task.name);
            for _ in 0..count {
                if cursor < num_work {
                    texts[work_indices[cursor]] = Some(text.clone());
                    cursor += 1;
                }
            }
        }
    } else {
        // One task per work hour, then append surplus into no-break hours.
        let mut parts: Vec<Vec<String>> = work_indices.iter().enumerate().map(|(i, _)| vec![locale.work_text(&tasks[i].name)]).collect();

        let surplus: Vec<String> = tasks[num_work..].iter().map(|t| locale.work_text(&t.name)).collect();
        let mut no_break_local: Vec<usize> = work_indices
            .iter()
            .enumerate()
            .filter(|&(_, &slot_idx)| !slots[slot_idx].has_break)
            .map(|(local_i, _)| local_i)
            .collect();

        if no_break_local.is_empty() {
            // Fallback so surplus task names are not dropped from the report.
            no_break_local = (0..num_work).collect();
        }

        let base = surplus.len() / no_break_local.len();
        let mut rem = surplus.len() % no_break_local.len();
        let mut iter = surplus.into_iter();

        for &local_i in &no_break_local {
            let count = base + if rem > 0 { 1 } else { 0 };
            rem = rem.saturating_sub(1);
            for _ in 0..count {
                if let Some(text) = iter.next() {
                    parts[local_i].push(text);
                }
            }
        }

        for (local_i, &slot_idx) in work_indices.iter().enumerate() {
            texts[slot_idx] = Some(parts[local_i].join(". "));
        }
    }

    texts
}

/// Builds rendered hourly rows from classified slots and assigned task texts.
///
/// - Pure break / empty slots → `break_label`
/// - Work without break → task text
/// - Work with break → `"{task}. {break_label}"`
pub(super) fn build_hourly_rows(slots: &[HourSlot], task_texts: &[Option<String>], break_label: &str) -> Vec<HourlyRow> {
    slots
        .iter()
        .enumerate()
        .map(|(i, slot)| {
            let description = if !slot.has_work {
                break_label.to_string()
            } else {
                let work = task_texts.get(i).and_then(|t| t.as_ref()).map(String::as_str).unwrap_or("");
                if slot.has_break {
                    if work.is_empty() {
                        break_label.to_string()
                    } else {
                        format!("{work}. {break_label}")
                    }
                } else if work.is_empty() {
                    break_label.to_string()
                } else {
                    work.to_string()
                }
            };

            HourlyRow {
                start: slot.start.format("%H:%M").to_string(),
                end: slot.end.format("%H:%M").to_string(),
                description,
            }
        })
        .collect()
}

#[cfg(test)]
mod hourly_tests {
    use super::*;
    use crate::libs::task::Task;
    use chrono::NaiveDate;

    fn dt(hour: u32, min: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2025, 1, 15).unwrap().and_hms_opt(hour, min, 0).unwrap()
    }

    fn interval(start_h: u32, start_m: u32, end_h: u32, end_m: u32) -> WorkInterval {
        let start = dt(start_h, start_m);
        let end = dt(end_h, end_m);
        WorkInterval {
            start,
            end,
            duration: end - start,
            pause_after: None,
        }
    }

    fn pause(start_h: u32, start_m: u32, end_h: u32, end_m: u32) -> Pause {
        let start = dt(start_h, start_m);
        let end = dt(end_h, end_m);
        Pause::detected(1, start, Some(end), Some(end - start))
    }

    fn task(name: &str) -> Task {
        Task::new(name, "", Some(0))
    }

    #[test]
    fn fewer_tasks_fill_contiguous_blocks() {
        let locale = Locale::for_language(Language::En);
        // 09:00–14:00 continuous work → 5 work hours
        let intervals = vec![interval(9, 0, 14, 0)];
        let slots = classify_hour_slots(dt(9, 0), dt(14, 0), &intervals, &[]);
        assert_eq!(slots.len(), 5);
        assert!(slots.iter().all(|s| s.has_work && !s.has_break));

        let tasks = vec![task("A"), task("B"), task("C")];
        let texts = assign_tasks_to_hour_slots(&tasks, &slots, locale);
        let names: Vec<&str> = texts.iter().map(|t| t.as_deref().unwrap()).collect();

        // 5 / 3 → base 1, extra 2 → A A B B C
        assert_eq!(
            names,
            vec![
                "Work on task [A]",
                "Work on task [A]",
                "Work on task [B]",
                "Work on task [B]",
                "Work on task [C]",
            ]
        );
    }

    #[test]
    fn surplus_tasks_go_to_no_break_hours() {
        let locale = Locale::for_language(Language::En);
        // work 09–12, break 12:00–12:30, work 12:30–14 → hours 09–11/13 no-break, 12 mixed
        let intervals = vec![interval(9, 0, 12, 0), interval(12, 30, 14, 0)];
        let interruptions = vec![pause(12, 0, 12, 30)];
        let slots = classify_hour_slots(dt(9, 0), dt(14, 0), &intervals, &interruptions);

        assert_eq!(slots.len(), 5);
        assert!(slots[0].has_work && !slots[0].has_break); // 09
        assert!(slots[1].has_work && !slots[1].has_break); // 10
        assert!(slots[2].has_work && !slots[2].has_break); // 11
        assert!(slots[3].has_work && slots[3].has_break); // 12 mixed
        assert!(slots[4].has_work && !slots[4].has_break); // 13

        // 5 tasks for 5 work hours → one each, no surplus
        let tasks = vec![task("A"), task("B"), task("C"), task("D"), task("E")];
        let texts = assign_tasks_to_hour_slots(&tasks, &slots, locale);
        assert_eq!(texts[3].as_deref(), Some("Work on task [D]"));

        // 6 tasks → surplus F must not land on mixed hour 12 (index 3)
        let tasks = vec![task("A"), task("B"), task("C"), task("D"), task("E"), task("F")];
        let texts = assign_tasks_to_hour_slots(&tasks, &slots, locale);
        assert_eq!(texts[3].as_deref(), Some("Work on task [D]"));
        assert!(texts.iter().enumerate().any(|(i, t)| i != 3 && t.as_ref().is_some_and(|s| s.contains("[F]"))));
        assert!(!texts[3].as_ref().unwrap().contains("[F]"));
    }

    #[test]
    fn surplus_distributed_across_no_break_hours() {
        let locale = Locale::for_language(Language::En);
        let intervals = vec![interval(9, 0, 12, 0)];
        let slots = classify_hour_slots(dt(9, 0), dt(12, 0), &intervals, &[]);
        let tasks = vec![task("A"), task("B"), task("C"), task("D"), task("E")];
        let texts = assign_tasks_to_hour_slots(&tasks, &slots, locale);

        // 5 tasks / 3 hours → base 1 each, then 2 surplus into first hours
        // base surplus=0, rem=2 → first two no-break hours get +1
        assert_eq!(texts[0].as_deref(), Some("Work on task [A]. Work on task [D]"));
        assert_eq!(texts[1].as_deref(), Some("Work on task [B]. Work on task [E]"));
        assert_eq!(texts[2].as_deref(), Some("Work on task [C]"));
    }

    #[test]
    fn pure_break_hour_has_only_break_label() {
        let locale = Locale::for_language(Language::En);
        let intervals = vec![interval(9, 0, 12, 0), interval(13, 0, 14, 0)];
        let interruptions = vec![pause(12, 0, 13, 0)];
        let slots = classify_hour_slots(dt(9, 0), dt(14, 0), &intervals, &interruptions);
        let tasks = vec![task("A"), task("B"), task("C")];
        let texts = assign_tasks_to_hour_slots(&tasks, &slots, locale);
        let rows = build_hourly_rows(&slots, &texts, locale.break_label);

        assert!(!slots[3].has_work && slots[3].has_break);
        assert_eq!(rows[3].description, "Break");
        assert!(texts[3].is_none());
    }

    #[test]
    fn mixed_hour_shows_task_and_break() {
        let locale = Locale::for_language(Language::En);
        let intervals = vec![interval(9, 0, 12, 0), interval(12, 30, 13, 0)];
        let interruptions = vec![pause(12, 0, 12, 30)];
        let slots = classify_hour_slots(dt(9, 0), dt(13, 0), &intervals, &interruptions);
        let tasks = vec![task("A"), task("B"), task("C"), task("D")];
        let texts = assign_tasks_to_hour_slots(&tasks, &slots, locale);
        let rows = build_hourly_rows(&slots, &texts, locale.break_label);

        assert!(slots[3].has_work && slots[3].has_break);
        assert_eq!(rows[3].description, "Work on task [D]. Break");
    }
}
