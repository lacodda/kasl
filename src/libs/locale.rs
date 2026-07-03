//! Built-in localization for hourly (SiServer-style) daily reports.
//!
//! This module centralizes every user-facing string, month name, weekday name
//! and date format used when rendering the hourly report. Previously all of
//! these were hardcoded in Russian inside [`crate::libs::export`]; they now live
//! in language-specific [`Locale`] tables so the report can be produced in
//! different languages selected via `config.report.language`.
//!
//! ## Scope
//!
//! Only the hourly report (`kasl export report --hourly`) consumes these
//! locales. Other exports (CSV/JSON/plain Excel) keep their existing English
//! wording and are intentionally not affected.
//!
//! ## Usage
//!
//! ```rust,no_run
//! use kasl::libs::locale::{Language, Locale};
//!
//! let locale = Locale::for_language(Language::from_code("en"));
//! assert_eq!(locale.months[0], "January");
//! ```

/// Supported report languages.
///
/// The default (and fallback for unknown codes) is [`Language::Ru`], which
/// preserves the original Russian wording of the report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    /// Russian — the historical default output.
    Ru,
    /// English.
    En,
}

impl Language {
    /// Parses a language code (case-insensitive) into a [`Language`].
    ///
    /// Recognizes `ru` and `en`. Any other value falls back to
    /// [`Language::Ru`] so that a typo never breaks report generation.
    pub fn from_code(code: &str) -> Language {
        match code.trim().to_ascii_lowercase().as_str() {
            "en" | "eng" | "english" => Language::En,
            _ => Language::Ru,
        }
    }
}

/// A complete set of localized strings for one language.
///
/// All fields are `&'static str` because the tables are compiled into the
/// binary. The `work_on_task` field contains a `{task}` placeholder that is
/// replaced with the actual task name at render time.
pub struct Locale {
    /// Report title shown in the merged header cell (e.g. "Отчет за день").
    pub report_title: &'static str,
    /// Label for a regular working day (e.g. "рабочий").
    pub day_type_working: &'static str,
    /// Caption for the workday-length header cell.
    pub workday_length: &'static str,
    /// Column header spanning the start/end time columns (e.g. "День").
    pub header_day: &'static str,
    /// Header for the interval start-time column.
    pub header_start: &'static str,
    /// Header for the interval end-time column.
    pub header_end: &'static str,
    /// Header for the optional "hours" column.
    pub header_hours: &'static str,
    /// Header for the optional "result" column.
    pub header_result: &'static str,
    /// Label for the total worked-hours footer row.
    pub total_worked: &'static str,
    /// Label preceding the free-form comment box.
    pub comment: &'static str,
    /// Work description template; `{task}` is replaced with the task name.
    pub work_on_task: &'static str,
    /// Generic work label used when a task has no name.
    pub work_generic: &'static str,
    /// Label written for hours (or parts of hours) spent on a break/pause.
    pub break_label: &'static str,
    /// Nominative month names, indexed 0 (January) through 11 (December).
    pub months: [&'static str; 12],
    /// Weekday names, indexed 0 (Monday) through 6 (Sunday).
    pub weekdays: [&'static str; 7],
    /// `chrono` date format string used for the date header cell.
    pub date_format: &'static str,
}

impl Locale {
    /// Returns the static [`Locale`] table for the given language.
    pub fn for_language(language: Language) -> &'static Locale {
        match language {
            Language::Ru => &RU,
            Language::En => &EN,
        }
    }

    /// Builds a work description for a task name using the `work_on_task`
    /// template, falling back to [`Locale::work_generic`] for empty names.
    pub fn work_text(&self, task_name: &str) -> String {
        if task_name.trim().is_empty() {
            self.work_generic.to_string()
        } else {
            self.work_on_task.replace("{task}", task_name)
        }
    }
}

/// Russian locale table (default).
static RU: Locale = Locale {
    report_title: "Отчет за день",
    day_type_working: "рабочий",
    workday_length: "Продолжительность рабочего дня",
    header_day: "День",
    header_start: "Начало",
    header_end: "Конец",
    header_hours: "Часы",
    header_result: "Результат",
    total_worked: "Отработано часов:",
    comment: "Комментарий:",
    work_on_task: "Работа по задаче [{task}]",
    work_generic: "Работа",
    break_label: "Перерыв",
    months: [
        "Январь",
        "Февраль",
        "Март",
        "Апрель",
        "Май",
        "Июнь",
        "Июль",
        "Август",
        "Сентябрь",
        "Октябрь",
        "Ноябрь",
        "Декабрь",
    ],
    weekdays: ["Понедельник", "Вторник", "Среда", "Четверг", "Пятница", "Суббота", "Воскресенье"],
    date_format: "%d.%m.%Y",
};

/// English locale table.
static EN: Locale = Locale {
    report_title: "Daily report",
    day_type_working: "working",
    workday_length: "Workday length",
    header_day: "Day",
    header_start: "Start",
    header_end: "End",
    header_hours: "Hours",
    header_result: "Result",
    total_worked: "Hours worked:",
    comment: "Comment:",
    work_on_task: "Work on task [{task}]",
    work_generic: "Work",
    break_label: "Break",
    months: [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ],
    weekdays: ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"],
    date_format: "%Y-%m-%d",
};
