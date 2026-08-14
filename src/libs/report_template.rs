//! Declarative, JSON-based design templates for the hourly daily report.
//!
//! The visual style of the hourly report (fonts, border/fill colors, column
//! widths, row heights and which sections are shown) used to be hardcoded in
//! [`crate::libs::export`]. This module externalizes that style into a
//! serializable [`ReportTemplate`] so it can be customized without recompiling.
//!

use crate::libs::data_storage::DataStorage;
use anyhow::Result;
use rust_xlsxwriter::Color;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Font specification for a single report element.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FontSpec {
    /// Font family name (e.g. "Verdana").
    pub name: String,
    /// Font size in points.
    pub size: f64,
    /// Whether the font is bold.
    #[serde(default)]
    pub bold: bool,
}

impl FontSpec {
    /// Convenience constructor.
    fn new(name: &str, size: f64, bold: bool) -> Self {
        Self {
            name: name.to_string(),
            size,
            bold,
        }
    }
}

/// The set of fonts used across the report's distinct elements.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateFonts {
    /// Font for the report title cell.
    pub title: FontSpec,
    /// Font for the month name cell.
    pub month: FontSpec,
    /// Font for the date cell.
    pub date: FontSpec,
    /// Font for table header cells.
    pub header: FontSpec,
    /// Font for the start/end time cells and totals.
    pub time: FontSpec,
}

/// A complete, serializable description of the hourly report's visual design.
///
/// All measurements mirror the values previously hardcoded for the SiServer
/// layout so that [`ReportTemplate::siserver`] reproduces the original look
/// byte-for-byte.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportTemplate {
    /// Fonts for the various report elements.
    pub fonts: TemplateFonts,
    /// Cell border color as `#RRGGBB`.
    pub border_color: String,
    /// Header/total fill color as `#RRGGBB`.
    pub header_fill: String,
    /// Widths of the five data columns (B..F).
    pub col_widths: [f64; 5],
    /// Height of each hourly data row.
    pub data_row_height: f64,
    /// Height of the title row.
    pub title_row_height: f64,
    /// Whether to render the "hours" column.
    pub show_hours_column: bool,
    /// Whether to render the "result" column.
    pub show_result_column: bool,
    /// Whether to render the comment label and comment box.
    pub show_comment: bool,
    /// Number of rows the comment box spans.
    pub comment_rows: u32,
}

impl ReportTemplate {
    /// Returns the built-in `siserver` template reproducing the original design.
    pub fn siserver() -> Self {
        Self {
            fonts: TemplateFonts {
                title: FontSpec::new("Verdana", 14.0, true),
                month: FontSpec::new("Verdana", 14.0, false),
                date: FontSpec::new("Verdana", 10.0, true),
                header: FontSpec::new("Verdana", 10.0, true),
                time: FontSpec::new("Verdana", 10.0, true),
            },
            border_color: "#333333".to_string(),
            header_fill: "#C0C0C0".to_string(),
            col_widths: [13.55, 13.55, 96.55, 14.89, 62.55],
            data_row_height: 126.0,
            title_row_height: 17.4,
            show_hours_column: true,
            show_result_column: true,
            show_comment: true,
            comment_rows: 11,
        }
    }

    /// Loads the named template, falling back to the built-in [`Self::siserver`].
    ///
    /// The lookup path is `<data>/report_templates/<name>.json`. Missing files
    /// or parse errors are non-fatal: the built-in default is returned instead.
    /// Regardless of the requested name, the default `siserver.json` is
    /// materialized on disk (if absent) as an editable example.
    pub fn load(name: &str) -> Self {
        // Best-effort: never fail report generation because of template I/O.
        let _ = Self::ensure_default_on_disk();

        match Self::templates_dir() {
            Ok(dir) => {
                let path = dir.join(format!("{}.json", name));
                match fs::read_to_string(&path) {
                    Ok(contents) => serde_json::from_str::<ReportTemplate>(&contents).unwrap_or_default(),
                    Err(_) => Self::siserver(),
                }
            }
            Err(_) => Self::siserver(),
        }
    }

    /// Writes the built-in `siserver.json` template to disk when it is missing.
    ///
    /// This gives users a ready-to-copy reference for authoring custom
    /// templates. Existing files are never overwritten.
    pub fn ensure_default_on_disk() -> Result<()> {
        let dir = Self::templates_dir()?;
        let path = dir.join("siserver.json");
        if !path.exists() {
            let json = serde_json::to_string_pretty(&Self::siserver())?;
            fs::write(&path, json)?;
        }
        Ok(())
    }

    /// Resolves (and creates) the `report_templates` directory under app data.
    fn templates_dir() -> Result<PathBuf> {
        let dir = DataStorage::new().get_path("report_templates")?;
        if !dir.exists() {
            fs::create_dir_all(&dir)?;
        }
        Ok(dir)
    }

    /// Parses a `#RRGGBB` (or `RRGGBB`) hex string into an xlsx [`Color`].
    ///
    /// Invalid strings fall back to the provided default color so a malformed
    /// template value never aborts rendering.
    pub fn parse_color(hex: &str, default: u32) -> Color {
        let trimmed = hex.trim().trim_start_matches('#');
        match u32::from_str_radix(trimmed, 16) {
            Ok(value) if trimmed.len() == 6 => Color::RGB(value),
            _ => Color::RGB(default),
        }
    }

    /// Border color as an xlsx [`Color`], defaulting to `#333333`.
    pub fn border(&self) -> Color {
        Self::parse_color(&self.border_color, 0x333333)
    }

    /// Header/total fill color as an xlsx [`Color`], defaulting to `#C0C0C0`.
    pub fn fill(&self) -> Color {
        Self::parse_color(&self.header_fill, 0xC0C0C0)
    }
}

impl Default for ReportTemplate {
    fn default() -> Self {
        Self::siserver()
    }
}
