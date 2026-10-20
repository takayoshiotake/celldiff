//! Positional CSV comparison with borrowed cell values and a streaming sink.
use crate::{AppError, input::CsvInput};
use csv::StringRecord;
use std::borrow::Cow;
use std::io::Read;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Modified,
    Added,
    Removed,
}
impl ChangeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Modified => "Modified",
            Self::Added => "Added",
            Self::Removed => "Removed",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct QuoteInfo {
    pub left: bool,
    pub right: bool,
}

#[derive(Debug)]
pub struct CellDifference<'a> {
    pub record: u64,
    pub column: usize,
    pub left: Option<&'a str>,
    pub right: Option<&'a str>,
    pub quotes: Option<QuoteInfo>,
}
fn csv_display(value: Option<&str>, quoted: bool) -> Option<Cow<'_, str>> {
    value.map(|value| {
        if quoted {
            Cow::Owned(format!("\"{}\"", value.replace('"', "\"\"")))
        } else {
            Cow::Borrowed(value)
        }
    })
}

impl<'a> CellDifference<'a> {
    pub fn differs(&self) -> bool {
        self.left != self.right || self.quotes.is_some_and(|q| q.left != q.right)
    }

    pub fn display_left(&self) -> Option<Cow<'a, str>> {
        csv_display(self.left, self.quotes.is_some_and(|q| q.left))
    }

    pub fn display_right(&self) -> Option<Cow<'a, str>> {
        csv_display(self.right, self.quotes.is_some_and(|q| q.right))
    }

    pub fn kind(&self) -> ChangeKind {
        match (self.left, self.right) {
            (None, Some(_)) => ChangeKind::Added,
            (Some(_), None) => ChangeKind::Removed,
            _ => ChangeKind::Modified,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Summary {
    pub left_records: u64,
    pub right_records: u64,
    pub changed_records: u64,
    pub changed_cells: u64,
    pub modified: u64,
    pub added: u64,
    pub removed: u64,
}

/// Visit changed cells only.
pub fn compare<R: Read, S: Read>(
    left: &mut CsvInput<R>,
    right: &mut CsvInput<S>,
    emit: impl FnMut(CellDifference<'_>) -> Result<(), AppError>,
) -> Result<Summary, AppError> {
    compare_impl(left, right, false, emit)
}

/// Visit every existing cell in row-major order, including unchanged values.
pub fn compare_all<R: Read, S: Read>(
    left: &mut CsvInput<R>,
    right: &mut CsvInput<S>,
    emit: impl FnMut(CellDifference<'_>) -> Result<(), AppError>,
) -> Result<Summary, AppError> {
    compare_impl(left, right, true, emit)
}

fn compare_impl<R: Read, S: Read>(
    left: &mut CsvInput<R>,
    right: &mut CsvInput<S>,
    include_unchanged: bool,
    mut emit: impl FnMut(CellDifference<'_>) -> Result<(), AppError>,
) -> Result<Summary, AppError> {
    if left.compares_quotes() != right.compares_quotes() {
        return Err(std::io::Error::other(
            "Both inputs must use the same quote comparison setting",
        )
        .into());
    }
    let compare_quotes = left.compares_quotes();
    let mut old = StringRecord::new();
    let mut new = StringRecord::new();
    let mut summary = Summary::default();
    let mut record = 0;
    loop {
        let old_number = left.read_record(&mut old)?;
        let new_number = right.read_record(&mut new)?;
        if old_number.is_none() && new_number.is_none() {
            break;
        }
        record += 1;
        summary.left_records += u64::from(old_number.is_some());
        summary.right_records += u64::from(new_number.is_some());
        let mut changed = false;
        let width = old.len().max(new.len());
        for index in 0..width {
            let old_value = old_number.and_then(|_| old.get(index));
            let new_value = new_number.and_then(|_| new.get(index));
            let quotes = compare_quotes.then(|| QuoteInfo {
                left: old_number.is_some() && left.is_quoted(index),
                right: new_number.is_some() && right.is_quoted(index),
            });
            let differs = old_value != new_value || quotes.is_some_and(|q| q.left != q.right);
            if !differs && !include_unchanged {
                continue;
            }
            let difference = CellDifference {
                record,
                column: index + 1,
                left: old_value,
                right: new_value,
                quotes,
            };
            if differs {
                match difference.kind() {
                    ChangeKind::Modified => summary.modified += 1,
                    ChangeKind::Added => summary.added += 1,
                    ChangeKind::Removed => summary.removed += 1,
                }
                summary.changed_cells += 1;
                changed = true;
            }
            emit(difference)?;
        }
        summary.changed_records += u64::from(changed);
    }
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    type Values = (u64, usize, Option<String>, Option<String>);
    fn run(old: &str, new: &str) -> (Summary, Vec<Values>) {
        let mut values = Vec::new();
        let summary = compare(
            &mut CsvInput::new(old.as_bytes(), "left"),
            &mut CsvInput::new(new.as_bytes(), "right"),
            |d| {
                values.push((
                    d.record,
                    d.column,
                    d.left.map(str::to_owned),
                    d.right.map(str::to_owned),
                ));
                Ok(())
            },
        )
        .unwrap();
        (summary, values)
    }
    #[test]
    fn equal_parsed_values_ignore_csv_quoting() {
        let (summary, values) = run("a,b\n", "\"a\",b\r\n");
        assert_eq!(summary.changed_cells, 0);
        assert!(values.is_empty());
    }
    #[test]
    fn preserves_zeros_spaces_and_case() {
        let (summary, values) = run("001, a,A\n", "1,a,a\n");
        assert_eq!(summary.modified, 3);
        assert_eq!(values[0].1, 1);
    }
    #[test]
    fn extra_columns_and_records_are_not_lost() {
        let (summary, values) = run("a,\nz\n", "a\n");
        assert_eq!(summary.removed, 2);
        assert_eq!(summary.left_records, 2);
        assert_eq!(summary.right_records, 1);
        assert_eq!(values[0], (1, 2, Some("".into()), None));
        assert_eq!(values[1], (2, 1, Some("z".into()), None));
        assert_eq!(run("a\n", "a,\nz\n").0.added, 2);
    }
    #[test]
    fn inserted_records_keep_positional_semantics() {
        let (summary, _) = run("a\nb\n", "x\na\nb\n");
        assert_eq!(summary.modified, 2);
        assert_eq!(summary.added, 1);
    }
    #[test]
    fn empty_inputs_and_multiline_record_numbers() {
        assert_eq!(run("", "").0, Summary::default());
        let (_, values) = run("\"a\nb\"\nx\n", "\"a\nb\"\ny\n");
        assert_eq!(values[0].0, 2);
    }
    #[test]
    fn sink_failure_stops_comparison() {
        let result = compare(
            &mut CsvInput::new(b"a\nb\n".as_slice(), "left"),
            &mut CsvInput::new(b"x\ny\n".as_slice(), "right"),
            |_| Err(std::io::Error::other("output failed").into()),
        );
        assert!(result.unwrap_err().to_string().contains("output failed"));
    }

    #[test]
    fn all_cells_keep_coordinates_across_equal_and_missing_cells() {
        let mut coordinates = Vec::new();
        let summary = compare_all(
            &mut CsvInput::new(b"id,name\n001,same\n002,\n".as_slice(), "left"),
            &mut CsvInput::new(b"id,name\n001,same\n002\n".as_slice(), "right"),
            |cell| {
                coordinates.push((
                    cell.record,
                    cell.column,
                    cell.left.map(str::to_owned),
                    cell.right.map(str::to_owned),
                ));
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(coordinates.len(), 6);
        assert_eq!(coordinates[0], (1, 1, Some("id".into()), Some("id".into())));
        assert_eq!(coordinates[5], (3, 2, Some("".into()), None));
        assert_eq!(summary.changed_cells, 1);
    }
}
