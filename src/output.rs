//! Constant-memory XLSX difference report, staged until successful completion.
use crate::{
    AppError,
    compare::{CellDifference, Summary},
    text,
};
use rust_xlsxwriter::{Color, Format, Workbook};
use similar::ChangeTag;
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;

pub const MAX_RECORDS: u64 = 1_048_576;
pub const MAX_COLUMNS: usize = 16_384;
pub const MAX_TEXT_UNITS: usize = 32_767;

fn fail(message: impl Into<String>) -> AppError {
    std::io::Error::other(message.into()).into()
}

pub fn validate_text(value: &str) -> Result<(), AppError> {
    if value.encode_utf16().count() > MAX_TEXT_UNITS {
        return Err(fail("Cell exceeds Excel's 32767 UTF-16-unit text limit"));
    }
    Ok(())
}

pub fn validate_position(record: u64, column: usize) -> Result<(), AppError> {
    if record == 0 || record > MAX_RECORDS || column == 0 || column > MAX_COLUMNS {
        return Err(fail(format!("Record {record}, column {column} exceeds Excel's 1048576-row / 16384-column bounds")));
    }
    Ok(())
}

pub fn validate_paths(
    before: &Path,
    after: &Path,
    output: &Path,
    force: bool,
) -> Result<(), AppError> {
    if !output
        .extension()
        .is_some_and(|e| e.to_str().is_some_and(|s| s.eq_ignore_ascii_case("xlsx")))
    {
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
    columns: usize,
    normal: Format,
    removed: Format,
    added: Format,
    cell: Format,
    modified_cell: Format,
    added_cell: Format,
    removed_cell: Format,
}

impl Report {
    pub fn new() -> Result<Self, AppError> {
        let mut workbook = Workbook::new();
        let sheet = workbook.add_worksheet_with_constant_memory();
        sheet.set_name("Differences")?;
        // No metadata header: CSV record 1, field 1 stays at Excel A1.
        Ok(Self {
            workbook,
            columns: 0,
            normal: Format::new(),
            removed: Format::new()
                .set_font_color(Color::RGB(0xB91C1C))
                .set_font_strikethrough(),
            added: Format::new()
                .set_font_color(Color::RGB(0x1D4ED8))
                .set_bold(),
            cell: Format::new().set_text_wrap(),
            modified_cell: Format::new().set_text_wrap().set_background_color(Color::RGB(0xFEF9C3)),
            added_cell: Format::new().set_text_wrap().set_background_color(Color::RGB(0xDBEAFE)),
            removed_cell: Format::new().set_text_wrap().set_background_color(Color::RGB(0xFEE2E2)),
        })
    }

    pub fn write(&mut self, cell: CellDifference<'_>) -> Result<(), AppError> {
        validate_position(cell.record, cell.column)?;
        if cell.column > self.columns {
            self.workbook.worksheet_from_index(0)?.set_column_range_width(self.columns as u16, (cell.column - 1) as u16, 24)?;
            self.columns = cell.column;
        }
        let differs = cell.before != cell.after;
        let row = (cell.record - 1) as u32;
        let column = (cell.column - 1) as u16;
        let error_at = |error| fail(format!("Record {}, column {}: {error}", cell.record, cell.column));
        for value in [cell.before, cell.after].into_iter().flatten() {
            validate_text(value).map_err(error_at)?;
        }
        if !differs {
            let sheet = self.workbook.worksheet_from_index(0)?;
            if let Some(value) = cell.after {
                if value.is_empty() {
                    sheet.write_blank(row, column, &self.cell)?;
                } else {
                    sheet.write_string_with_format(row, column, value, &self.cell)?;
                }
            }
            return Ok(());
        }
        let parts = text::segments(cell.before.unwrap_or(""), cell.after.unwrap_or(""));
        let combined: String = parts.iter().map(|part| part.value.as_str()).collect();
        // Replacements show removed + inserted text in the same cell.
        // Validate the rendered text too: two valid inputs may exceed the XLSX limit together.
        validate_text(&combined).map_err(error_at)?;
        let format = match cell.kind() {
            crate::compare::ChangeKind::Modified => &self.modified_cell,
            crate::compare::ChangeKind::Added => &self.added_cell,
            crate::compare::ChangeKind::Removed => &self.removed_cell,
        };
        let fragments: Vec<_> = parts.iter().map(|part| {
            let font = match part.tag {
                ChangeTag::Equal => &self.normal,
                ChangeTag::Delete => &self.removed,
                ChangeTag::Insert => &self.added,
            };
            (font, part.value.as_str())
        }).collect();
        let sheet = self.workbook.worksheet_from_index(0)?;
        if fragments.len() >= 2 {
            sheet.write_rich_string_with_format(row, column, &fragments, format)?;
        } else if let Some((font, _)) = fragments.first() {
            sheet.write_string_with_format(row, column, &combined, &font.merge(format))?;
        } else {
            sheet.write_blank(row, column, format)?;
        }
        Ok(())
    }

    pub fn save(
        mut self,
        path: &Path,
        force: bool,
        summary: Summary,
        before: &Path,
        after: &Path,
    ) -> Result<(), AppError> {
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
            (
                "Result",
                if summary.changed_cells == 0 {
                    "No differences"
                } else {
                    "Differences found"
                }
                .to_string(),
            ),
            (
                "Comparison",
                "Positional; first record included; UTF-8; no normalization".into(),
            ),
            ("Removed text", "Red with strikethrough within each original cell".into()),
            ("Added text", "Blue and bold within each original cell".into()),
            (
                "Empty vs missing",
                "Blue fill: added cell; red fill: removed cell; yellow: modified".into(),
            ),
            ("Character unit", "Unicode grapheme cluster".into()),
            ("Layout", "CSV coordinates preserved; unchanged cells included; no extra header".into()),
            ("Cell text", "Removed and inserted segments are combined; this is a visual diff".into()),
        ];
        for (row, (label, value)) in entries.iter().enumerate() {
            sheet.write_string(row as u32, 0, *label)?;
            sheet.write_string(row as u32, 1, value)?;
        }
        let parent: PathBuf = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."))
            .into();
        let mut staged = NamedTempFile::new_in(parent)?;
        self.workbook.save_to_writer(staged.as_file_mut())?;
        staged.as_file().sync_all()?;
        // Drop worksheet temp files before publishing the output.
        drop(self.workbook);
        if force {
            staged
                .persist(path)
                .map_err(|e| fail(format!("Cannot replace output: {}", e.error)))?;
        } else {
            staged.persist_noclobber(path).map_err(|e| {
                fail(format!(
                    "Cannot publish output without overwriting: {}",
                    e.error
                ))
            })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[test]
    fn limits_are_checked_without_truncation() {
        assert!(validate_position(MAX_RECORDS, MAX_COLUMNS).is_ok());
        assert!(validate_position(MAX_RECORDS + 1, 1).is_err());
        assert!(validate_position(1, MAX_COLUMNS + 1).is_err());
        assert!(validate_position(0, 1).is_err());
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
