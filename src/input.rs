//! Streaming CSV input decoded to UTF-8. Every record, including the first, is data.

use std::error::Error;
use std::fmt;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::encoding::{DecodedReader, InputEncoding};
use crate::quote_tracking::TrackedReader;
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
    reader: Reader<TrackedReader<R>>,
    input: String,
    records_read: u64,
    quoted: Vec<bool>,
}

impl<R: Read> CsvInput<R> {
    pub fn new(reader: R, input: impl Into<String>) -> Self {
        Self::new_with_quotes(reader, input, false)
    }

    pub fn new_with_quotes(reader: R, input: impl Into<String>, compare_quotes: bool) -> Self {
        Self {
            reader: ReaderBuilder::new()
                .has_headers(false)
                .flexible(true)
                .trim(Trim::None)
                .from_reader(TrackedReader::new(reader, compare_quotes)),
            input: input.into(),
            records_read: 0,
            quoted: Vec::new(),
        }
    }

    pub fn compares_quotes(&self) -> bool {
        self.reader.get_ref().enabled()
    }

    pub fn is_quoted(&self, column: usize) -> bool {
        self.quoted.get(column).copied().unwrap_or(false)
    }

    /// Returns the 1-based record number, or None at EOF.
    /// The buffer is overwritten; borrowed cells must not outlive the next read.
    pub fn read_record(&mut self, buffer: &mut StringRecord) -> Result<Option<u64>, InputError> {
        self.quoted.clear();
        match self.reader.read_record(buffer) {
            Ok(true) => {
                if self.compares_quotes() {
                    let end = self.reader.position().byte();
                    self.reader
                        .get_mut()
                        .record_quotes(end, &mut self.quoted)
                        .map_err(|source| InputError {
                            input: self.input.clone(),
                            source: source.into(),
                        })?;
                    if self.quoted.len() != buffer.len() {
                        return Err(InputError {
                            input: self.input.clone(),
                            source: std::io::Error::other(
                                "CSV quote metadata does not match parsed fields",
                            )
                            .into(),
                        });
                    }
                }
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
        Self::open_encoded_with_quotes(path, encoding, false)
    }

    pub fn open_encoded_with_quotes(
        path: impl AsRef<Path>,
        encoding: InputEncoding,
        compare_quotes: bool,
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
        Ok(Self::new_with_quotes(reader, input, compare_quotes))
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

#[cfg(test)]
mod quote_tests {
    use super::*;

    fn parse<R: Read>(source: R) -> Vec<(Vec<String>, Vec<bool>)> {
        let mut reader = CsvInput::new_with_quotes(source, "quotes.csv", true);
        let mut record = StringRecord::new();
        let mut rows = Vec::new();
        while reader.read_record(&mut record).unwrap().is_some() {
            rows.push((
                record.iter().map(str::to_owned).collect(),
                (0..record.len()).map(|i| reader.is_quoted(i)).collect(),
            ));
        }
        rows
    }

    struct Short<'a>(&'a [u8]);
    impl Read for Short<'_> {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            let size = self.0.len().min(out.len()).min(1);
            out[..size].copy_from_slice(&self.0[..size]);
            self.0 = &self.0[size..];
            Ok(size)
        }
    }

    #[test]
    fn tracks_bom_blank_lines_escaped_quotes_and_multiline_fields() {
        let data = b"\xef\xbb\xbf\r\n\n1,\"1\",,\"\"\r\n\n\"a,b\",\"a\r\nb\",\"a\"\"b\",z\nlast,\n";
        let rows = parse(data.as_slice());
        assert_eq!(
            rows,
            vec![
                (
                    vec!["1".into(), "1".into(), "".into(), "".into()],
                    vec![false, true, false, true]
                ),
                (
                    vec!["a,b".into(), "a\r\nb".into(), "a\"b".into(), "z".into()],
                    vec![true, true, true, false]
                ),
                (vec!["last".into(), "".into()], vec![false, false]),
            ]
        );
        assert_eq!(parse(Short(&data[3..])), rows);
        // csv-core treats a BOM split across its initial reads as data. The
        // CLI's DecodedReader removes the BOM first; preserve the generic
        // CsvInput parser behavior without introducing a metadata error.
        let split = parse(Short(data));
        assert_eq!(split[0], (vec!["\u{feff}".into()], vec![false]));
        assert_eq!(&split[1..], rows.as_slice());
    }

    #[test]
    fn tracks_records_across_buffers_without_leaking_previous_flags() {
        let text = format!(
            "plain,\"{}\"\n\"quoted\"\nlast,\n",
            "a,\"\"b\n".repeat(5000)
        );
        let rows = parse(text.as_bytes());
        assert_eq!(rows[0].1, vec![false, true]);
        assert_eq!(rows[1].1, vec![true]);
        assert_eq!(rows[2].1, vec![false, false]);
        assert_eq!(rows[0].0[1], "a,\"b\n".repeat(5000));
    }

    #[test]
    fn quote_tracking_preserves_csv_parser_acceptance_for_short_inputs() {
        let alphabet = b"a,\"\r\n ";
        // Exercise permissive CSV forms too, including unclosed quotes and
        // characters following a closing quote. csv remains the value parser.
        for length in 0..=5 {
            for mut number in 0..alphabet.len().pow(length) {
                let mut data = vec![0; length as usize];
                for byte in &mut data {
                    *byte = alphabet[number % alphabet.len()];
                    number /= alphabet.len();
                }
                let actual = parse(data.as_slice());
                let mut baseline = CsvInput::new(data.as_slice(), "baseline");
                let mut buffer = StringRecord::new();
                let mut expected = Vec::new();
                while baseline.read_record(&mut buffer).unwrap().is_some() {
                    expected.push(buffer.iter().map(str::to_owned).collect::<Vec<_>>());
                }
                assert_eq!(
                    actual.iter().map(|r| r.0.clone()).collect::<Vec<_>>(),
                    expected,
                    "input={:?}",
                    String::from_utf8_lossy(&data)
                );
            }
        }
    }
}
