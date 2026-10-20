//! Streaming plain-text differences, independent of Excel limits and file output.
use crate::compare::{CellDifference, Summary};
use clap::ValueEnum;
use std::io::{self, Write};

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum AddressStyle {
    #[default]
    A1,
    Rc,
}

fn address(style: AddressStyle, row: u64, mut column: usize) -> String {
    match style {
        AddressStyle::Rc => format!("row={row} column={column}"),
        AddressStyle::A1 => {
            let mut letters = Vec::new();
            while column > 0 {
                column -= 1;
                letters.push(char::from(b'A' + (column % 26) as u8));
                column /= 26;
            }
            let letters: String = letters.into_iter().rev().collect();
            format!("{letters}{row}")
        }
    }
}

fn value(writer: &mut impl Write, value: Option<&str>) -> io::Result<()> {
    match value {
        // Rust's quoted debug representation escapes controls, quotes and backslashes.
        Some(text) => write!(writer, "{text:?}"),
        None => write!(writer, "<missing>"),
    }
}

pub struct TextReport<W: Write> {
    writer: W,
    oneline: bool,
    address_style: AddressStyle,
}

impl<W: Write> TextReport<W> {
    pub fn new(writer: W, oneline: bool, address_style: AddressStyle) -> Self {
        Self {
            writer,
            oneline,
            address_style,
        }
    }

    pub fn write(&mut self, cell: CellDifference<'_>) -> io::Result<()> {
        let address = address(self.address_style, cell.record, cell.column);
        if self.oneline {
            write!(self.writer, "{address}: ")?;
            value(&mut self.writer, cell.left)?;
            write!(self.writer, " -> ")?;
            value(&mut self.writer, cell.right)?;
            writeln!(self.writer)?;
        } else {
            writeln!(self.writer, "{address}:")?;
            write!(self.writer, "  - ")?;
            value(&mut self.writer, cell.left)?;
            write!(self.writer, "\n  + ")?;
            value(&mut self.writer, cell.right)?;
            writeln!(self.writer, "\n")?;
        }
        Ok(())
    }

    /// Call only after successful comparison. Flush failures are also fatal.
    pub fn finish(&mut self, summary: Summary) -> io::Result<()> {
        if summary.changed_cells == 0 {
            writeln!(self.writer, "No differences.")?;
        } else {
            if self.oneline {
                writeln!(self.writer)?;
            }
            writeln!(
                self.writer,
                "Changed cells: {} (modified={}, added={}, removed={})",
                summary.changed_cells, summary.modified, summary.added, summary.removed
            )?;
        }
        self.writer.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_conversion_has_no_excel_limit() {
        for (column, expected) in [
            (1, "A1"),
            (26, "Z1"),
            (27, "AA1"),
            (52, "AZ1"),
            (53, "BA1"),
            (16384, "XFD1"),
            (16385, "XFE1"),
        ] {
            assert_eq!(address(AddressStyle::A1, 1, column), expected);
        }
        assert_eq!(address(AddressStyle::A1, 1048577, 27), "AA1048577");
        assert_eq!(address(AddressStyle::Rc, 123, 27), "row=123 column=27");
    }

    struct Broken {
        fail_flush: bool,
    }
    impl Write for Broken {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.fail_flush {
                Ok(bytes.len())
            } else {
                Err(io::ErrorKind::BrokenPipe.into())
            }
        }
        fn flush(&mut self) -> io::Result<()> {
            Err(io::ErrorKind::BrokenPipe.into())
        }
    }

    #[test]
    fn write_and_flush_failures_are_returned() {
        let mut report = TextReport::new(Broken { fail_flush: false }, true, AddressStyle::A1);
        assert_eq!(
            report
                .write(CellDifference {
                    record: 1,
                    column: 1,
                    left: Some("a"),
                    right: Some("b")
                })
                .unwrap_err()
                .kind(),
            io::ErrorKind::BrokenPipe
        );
        let mut report = TextReport::new(Broken { fail_flush: true }, false, AddressStyle::Rc);
        assert_eq!(
            report.finish(Summary::default()).unwrap_err().kind(),
            io::ErrorKind::BrokenPipe
        );
    }
}
