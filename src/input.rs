//! Streaming CSV input decoded to UTF-8. Every record, including the first, is data.

use std::error::Error;
use std::fmt;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::encoding::{DecodedReader, InputEncoding};
use csv::{Reader, ReaderBuilder, StringRecord, Trim};

#[derive(Debug)]
pub struct InputError {
    input: String,
    source: csv::Error,
}

impl InputError {
    pub fn input(&self) -> &str {
        &self.input
    }

    pub fn position(&self) -> Option<&csv::Position> {
        self.source.position()
    }
}

impl fmt::Display for InputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.input, self.source)
    }
}

impl Error for InputError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.source)
    }
}

/// A bounded-memory reader. The caller owns and reuses its record buffer.
/// Empty physical lines are skipped by the CSV parser.
pub struct CsvInput<R: Read> {
    reader: Reader<R>,
    input: String,
    records_read: u64,
}

impl<R: Read> CsvInput<R> {
    pub fn new(reader: R, input: impl Into<String>) -> Self {
        Self {
            reader: ReaderBuilder::new()
                .has_headers(false)
                .flexible(true)
                .trim(Trim::None)
                .from_reader(reader),
            input: input.into(),
            records_read: 0,
        }
    }

    /// Returns the 1-based record number, or None at EOF.
    /// The buffer is overwritten; borrowed cells must not outlive the next read.
    pub fn read_record(&mut self, buffer: &mut StringRecord) -> Result<Option<u64>, InputError> {
        match self.reader.read_record(buffer) {
            Ok(true) => {
                self.records_read += 1;
                Ok(Some(self.records_read))
            }
            Ok(false) => Ok(None),
            Err(source) => Err(InputError {
                input: self.input.clone(),
                source,
            }),
        }
    }
}

impl CsvInput<File> {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, InputError> {
        let path = path.as_ref();
        let input = path.display().to_string();
        let file = File::open(path).map_err(|source| InputError {
            input: input.clone(),
            source: source.into(),
        })?;
        Ok(Self::new(file, input))
    }
}

impl CsvInput<DecodedReader<File>> {
    pub fn open_encoded(
        path: impl AsRef<Path>,
        encoding: InputEncoding,
    ) -> Result<Self, InputError> {
        let path = path.as_ref();
        let input = path.display().to_string();
        let open = || {
            let file = File::open(path)?;
            DecodedReader::new(file, encoding)
        };
        let reader = open().map_err(|source: std::io::Error| InputError {
            input: input.clone(),
            source: source.into(),
        })?;
        Ok(Self::new(reader, input))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn records(data: &[u8]) -> Vec<Vec<String>> {
        let mut input = CsvInput::new(data, "left.csv");
        let mut buffer = StringRecord::new();
        let mut result = Vec::new();
        while input.read_record(&mut buffer).unwrap().is_some() {
            result.push(buffer.iter().map(str::to_owned).collect());
        }
        result
    }

    #[test]
    fn includes_first_record_and_preserves_values() {
        assert_eq!(
            records("id,name\r\n001, 東京 \r\n".as_bytes()),
            vec![vec!["id", "name"], vec!["001", " 東京 "]]
        );
    }

    #[test]
    fn handles_quoted_commas_newlines_and_quotes() {
        assert_eq!(
            records(b"\"a,b\",\"a\nb\",\"a\"\"b\"\n"),
            vec![vec!["a,b", "a\nb", "a\"b"]]
        );
    }

    #[test]
    fn allows_variable_field_counts_and_distinguishes_empty_from_missing() {
        assert_eq!(
            records(b"a,b\na\na,\n,a,\n"),
            vec![vec!["a", "b"], vec!["a"], vec!["a", ""], vec!["", "a", ""]]
        );
    }

    #[test]
    fn accepts_utf8_bom() {
        assert_eq!(
            records(b"\xef\xbb\xbf001,name\n"),
            vec![vec!["001", "name"]]
        );
    }

    #[test]
    fn empty_input_has_no_records() {
        assert!(records(b"").is_empty());
    }

    #[test]
    fn skips_empty_physical_lines_but_keeps_explicit_empty_fields() {
        assert_eq!(records(b"\n\r\n\"\"\n,\n"), vec![vec![""], vec!["", ""]]);
    }

    #[test]
    fn counts_logical_records_and_reuses_buffer_without_stale_fields() {
        let mut input = CsvInput::new(b"\"a\nb\",c\nz\n".as_slice(), "right.csv");
        let mut buffer = StringRecord::new();
        assert_eq!(input.read_record(&mut buffer).unwrap(), Some(1));
        assert_eq!(buffer.len(), 2);
        assert_eq!(input.read_record(&mut buffer).unwrap(), Some(2));
        assert_eq!(buffer.iter().collect::<Vec<_>>(), vec!["z"]);
        assert_eq!(input.read_record(&mut buffer).unwrap(), None);
    }

    #[test]
    fn invalid_utf8_reports_input_and_position() {
        let mut input = CsvInput::new(b"ok\n\xff\n".as_slice(), "right.csv");
        let mut buffer = StringRecord::new();
        input.read_record(&mut buffer).unwrap();
        let error = input.read_record(&mut buffer).unwrap_err();
        assert_eq!(error.input(), "right.csv");
        assert_eq!(error.position().unwrap().record(), 1);
        assert!(error.to_string().contains("right.csv"));
        assert!(error.source().is_some());
    }

    #[test]
    fn io_failure_retains_input_and_source() {
        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("read failed"))
            }
        }
        let mut input = CsvInput::new(Broken, "left.csv");
        let error = input.read_record(&mut StringRecord::new()).unwrap_err();
        assert_eq!(error.input(), "left.csv");
        assert!(error.to_string().contains("read failed"));
        assert!(error.source().is_some());
    }
}
