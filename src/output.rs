//! Constant-memory XLSX difference report, staged until successful completion.
use std::path::{Path, PathBuf};
use rust_xlsxwriter::{Color, Format, Workbook, Worksheet};
use similar::ChangeTag;
use tempfile::NamedTempFile;
use crate::{AppError, compare::{CellDifference, Summary}, text};

pub const MAX_DIFFERENCES: u64 = 1_048_575;
pub const MAX_TEXT_UNITS: usize = 32_767;
const MAX_EXACT_INTEGER: u64 = 9_007_199_254_740_991;

fn fail(message: impl Into<String>) -> AppError {
    std::io::Error::other(message.into()).into()
}

pub fn validate_text(value: &str) -> Result<(), AppError> {
    if value.encode_utf16().count() > MAX_TEXT_UNITS {
        return Err(fail("Cell exceeds Excel's 32767 UTF-16-unit text limit"));
    }
    Ok(())
}

pub fn validate_row(row: u64) -> Result<(), AppError> {
    if row > MAX_DIFFERENCES {
        return Err(fail("Report exceeds Excel's row limit (1048575 differences)"));
    }
    Ok(())
}

pub fn validate_paths(before: &Path, after: &Path, output: &Path, force: bool) -> Result<(), AppError> {
    if !output.extension().is_some_and(|e| e.to_str().is_some_and(|s| s.eq_ignore_ascii_case("xlsx"))) {
        return Err(fail("Output must have an .xlsx extension"));
    }
    // Includes hard links and symlinks, rather than comparing path strings.
    if output.exists() {
        for input in [before, after] {
            if same_file::is_same_file(input, output)? {
                return Err(fail("Output must not refer to either input file"));
            }
        }
        if !force {
            return Err(fail("Output already exists; use --force to replace it"));
        }
        if !output.is_file() {
            return Err(fail("Output is not a regular file"));
        }
    }
    Ok(())
}

pub struct Report {
    workbook: Workbook,
    rows: u64,
    normal: Format,
    removed: Format,
    added: Format,
    cell: Format,
}

impl Report {
    pub fn new() -> Result<Self, AppError> {
        let mut workbook = Workbook::new();
        let sheet = workbook.add_worksheet_with_constant_memory();
        sheet.set_name("Differences")?;
        sheet.set_freeze_panes(1, 0)?;
        for col in 0..7 {
            sheet.set_column_width(col, if col >= 5 { 45 } else { 18 })?;
        }
        let header = Format::new().set_bold().set_background_color(Color::RGB(0xE5E7EB));
        for (column, name) in ["Record", "Column", "Kind", "Before present", "After present", "Before", "After"].iter().enumerate() {
            sheet.write_string_with_format(0, column as u16, *name, &header)?;
        }
        Ok(Self {
            workbook,
            rows: 0,
            normal: Format::new(),
            removed: Format::new().set_font_color(Color::RGB(0xB91C1C)).set_font_strikethrough(),
            added: Format::new().set_font_color(Color::RGB(0x1D4ED8)).set_bold(),
            cell: Format::new().set_text_wrap(),
        })
    }

    pub fn write(&mut self, difference: CellDifference<'_>) -> Result<(), AppError> {
        validate_row(self.rows + 1)?;
        for value in [difference.before, difference.after].into_iter().flatten() {
            validate_text(value).map_err(|error| fail(format!("Record {}, column {}: {error}", difference.record, difference.column)))?;
        }
        let parts = text::segments(difference.before.unwrap_or(""), difference.after.unwrap_or(""));
        let row = (self.rows + 1) as u32;
        let sheet = self.workbook.worksheet_from_index(0)?;
        write_index(sheet, row, 0, difference.record)?;
        write_index(sheet, row, 1, difference.column as u64)?;
        sheet.write_string(row, 2, difference.kind().as_str())?;
        sheet.write_boolean(row, 3, difference.before.is_some())?;
        sheet.write_boolean(row, 4, difference.after.is_some())?;
        for (column, value, excluded) in [(5, difference.before, ChangeTag::Insert), (6, difference.after, ChangeTag::Delete)] {
            if let Some(value) = value {
                let fragments: Vec<_> = parts.iter().filter(|p| p.tag != excluded).map(|p| {
                    let format = match p.tag {
                        ChangeTag::Equal => &self.normal,
                        ChangeTag::Delete => &self.removed,
                        ChangeTag::Insert => &self.added,
                    };
                    (format, p.value.as_str())
                }).collect();
                if fragments.len() >= 2 {
                    sheet.write_rich_string_with_format(row, column, &fragments, &self.cell)?;
                } else if let Some((format, _)) = fragments.first() {
                    sheet.write_string_with_format(row, column, value, &format.merge(&self.cell))?;
                } else {
                    // Presence columns distinguish present-empty from missing.
                    sheet.write_blank(row, column, &self.cell)?;
                }
            }
        }
        self.rows += 1;
        Ok(())
    }

    pub fn save(mut self, path: &Path, force: bool, summary: Summary, before: &Path, after: &Path) -> Result<(), AppError> {
        let sheet = self.workbook.add_worksheet();
        sheet.set_name("Summary")?;
        sheet.set_column_width(0, 24)?;
        sheet.set_column_width(1, 70)?;
        let entries = [
            ("Before input", before.display().to_string()),
            ("After input", after.display().to_string()),
            ("Before records", summary.before_records.to_string()),
            ("After records", summary.after_records.to_string()),
            ("Changed records", summary.changed_records.to_string()),
            ("Changed cells", summary.changed_cells.to_string()),
            ("Modified cells", summary.modified.to_string()),
            ("Added cells", summary.added.to_string()),
            ("Removed cells", summary.removed.to_string()),
            ("Result", if summary.changed_cells == 0 { "No differences" } else { "Differences found" }.to_string()),
            ("Comparison", "Positional; first record included; UTF-8; no normalization".into()),
            ("Removed text", "Red with strikethrough in Before".into()),
            ("Added text", "Blue and bold in After".into()),
            ("Empty vs missing", "See Before present and After present columns".into()),
            ("Character unit", "Unicode grapheme cluster".into()),
        ];
        for (row, (label, value)) in entries.iter().enumerate() {
            sheet.write_string(row as u32, 0, *label)?;
            sheet.write_string(row as u32, 1, value)?;
        }
        let parent: PathBuf = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new(".")).into();
        let mut staged = NamedTempFile::new_in(parent)?;
        self.workbook.save_to_writer(staged.as_file_mut())?;
        staged.as_file().sync_all()?;
        // Drop worksheet temp files before publishing the output.
        drop(self.workbook);
        if force {
            staged.persist(path).map_err(|e| fail(format!("Cannot replace output: {}", e.error)))?;
        } else {
            staged.persist_noclobber(path).map_err(|e| fail(format!("Cannot publish output without overwriting: {}", e.error)))?;
        }
        Ok(())
    }
}

fn write_index(sheet: &mut Worksheet, row: u32, column: u16, value: u64) -> Result<(), AppError> {
    if value <= MAX_EXACT_INTEGER {
        sheet.write_number(row, column, value as f64)?;
    } else {
        sheet.write_string(row, column, value.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[test]
    fn limits_are_checked_without_truncation() {
        assert!(validate_row(MAX_DIFFERENCES).is_ok());
        assert!(validate_row(MAX_DIFFERENCES + 1).is_err());
        assert!(validate_text(&"a".repeat(MAX_TEXT_UNITS)).is_ok());
        assert!(validate_text(&"a".repeat(MAX_TEXT_UNITS + 1)).is_err());
        assert!(validate_text(&"😀".repeat(16384)).is_err());
    }
    #[test]
    fn force_cannot_overwrite_input_or_a_hard_link_to_it() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("old.xlsx");
        let new = dir.path().join("new.csv");
        fs::write(&old, "a").unwrap();
        fs::write(&new, "b").unwrap();
        assert!(validate_paths(&old, &new, &old, true).is_err());
        let link = dir.path().join("link.xlsx");
        fs::hard_link(&old, &link).unwrap();
        assert!(validate_paths(&old, &new, &link, true).is_err());
        assert_eq!(fs::read_to_string(old).unwrap(), "a");
    }
}
