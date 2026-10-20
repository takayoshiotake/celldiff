//! Retain only the current decoded CSV record plus parser read-ahead.
use std::io::{self, Read};

pub(crate) struct TrackedReader<R> {
    source: R,
    enabled: bool,
    bytes: Vec<u8>,
    base: u64,
    consumed: usize,
    bom_handled: bool,
}

impl<R: Read> TrackedReader<R> {
    pub(crate) fn new(source: R, enabled: bool) -> Self {
        Self {
            source,
            enabled,
            bytes: Vec::new(),
            base: 0,
            consumed: 0,
            bom_handled: false,
        }
    }

    pub(crate) fn enabled(&self) -> bool {
        self.enabled
    }

    pub(crate) fn record_quotes(&mut self, end: u64, flags: &mut Vec<bool>) -> io::Result<()> {
        if !self.enabled {
            return Ok(());
        }
        let stop = end
            .checked_sub(self.base)
            .and_then(|n| usize::try_from(n).ok())
            .ok_or_else(|| io::Error::other("Invalid CSV record byte position"))?;
        let raw = self
            .bytes
            .get(self.consumed..stop)
            .ok_or_else(|| io::Error::other("CSV record is outside the tracking buffer"))?;
        quote_flags(
            raw,
            self.bom_handled && self.base == 0 && self.consumed == 0,
            flags,
        );
        self.consumed = stop;
        Ok(())
    }
}

impl<R: Read> Read for TrackedReader<R> {
    fn read(&mut self, destination: &mut [u8]) -> io::Result<usize> {
        let read = self.source.read(destination)?;
        if self.enabled {
            // csv-core strips a UTF-8 BOM only when it is present in its first
            // input chunk. Match that behavior even for unusual short readers.
            if self.base == 0 && self.bytes.is_empty() {
                self.bom_handled = destination[..read].starts_with(b"\xef\xbb\xbf");
            }
            // Compact once per source read, not once per short CSV record.
            self.bytes.drain(..self.consumed);
            self.base += self.consumed as u64;
            self.consumed = 0;
            self.bytes.extend_from_slice(&destination[..read]);
        }
        Ok(read)
    }
}

// Values and record boundaries always come from csv::Reader. This scanner only
// tracks whether each field began with a quote, following its default grammar.
fn quote_flags(raw: &[u8], first: bool, flags: &mut Vec<bool>) {
    let mut index = if first && raw.starts_with(b"\xef\xbb\xbf") {
        3
    } else {
        0
    };
    while raw.get(index).is_some_and(|b| matches!(b, b'\r' | b'\n')) {
        index += 1;
    }
    if index == raw.len() {
        return;
    }
    loop {
        let quoted = raw.get(index) == Some(&b'"');
        flags.push(quoted);
        if quoted {
            index += 1;
            while index < raw.len() {
                if raw[index] == b'"' {
                    index += 1;
                    if raw.get(index) == Some(&b'"') {
                        index += 1;
                        continue;
                    }
                    break;
                }
                index += 1;
            }
        }
        // csv accepts trailing characters after a closing quote and literal
        // quotes within an unquoted field; neither begins a new quoted region.
        while raw
            .get(index)
            .is_some_and(|b| !matches!(b, b',' | b'\r' | b'\n'))
        {
            index += 1;
        }
        if raw.get(index) != Some(&b',') {
            break;
        }
        index += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completed_records_are_discarded_before_more_input_is_read() {
        let data = b"a,\"b\"\n".repeat(100000);
        let mut csv = csv::ReaderBuilder::new()
            .has_headers(false)
            .from_reader(TrackedReader::new(data.as_slice(), true));
        let mut record = csv::StringRecord::new();
        let mut flags = Vec::new();
        while csv.read_record(&mut record).unwrap() {
            flags.clear();
            let end = csv.position().byte();
            csv.get_mut().record_quotes(end, &mut flags).unwrap();
            assert_eq!(flags, vec![false, true]);
            assert!(csv.get_ref().bytes.len() <= 16384);
        }
    }
}
