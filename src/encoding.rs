//! Explicit encoding selection and strict, bounded-memory UTF-8 transcoding.
use encoding_rs::{Decoder, DecoderResult, Encoding};
use std::fmt;
use std::io::{self, Read};
use std::str::FromStr;

// This is also the complete CLI allowlist. Never accept an unlisted alias silently.
pub const ENCODINGS: &[(&str, &[&str])] = &[
    ("UTF-8", &["utf8"]),
    ("UTF-16LE", &["utf16le"]),
    ("UTF-16BE", &["utf16be"]),
    ("Shift_JIS", &["cp932", "windows-31j", "shift-jis", "sjis"]),
    ("EUC-JP", &["eucjp"]),
    ("ISO-2022-JP", &["iso2022jp"]),
    ("GBK", &["cp936"]),
    ("GB18030", &[]),
    ("Big5", &["cp950"]),
    ("EUC-KR", &["cp949", "windows-949"]),
    ("windows-1250", &["cp1250"]),
    ("windows-1251", &["cp1251"]),
    (
        "windows-1252",
        &["cp1252", "iso-8859-1", "latin1", "ascii", "us-ascii"],
    ),
    ("windows-1253", &["cp1253"]),
    ("windows-1254", &["cp1254", "iso-8859-9"]),
    ("windows-1255", &["cp1255"]),
    ("windows-1256", &["cp1256"]),
    ("windows-1257", &["cp1257"]),
    ("windows-1258", &["cp1258"]),
    ("windows-874", &["cp874", "tis-620", "iso-8859-11"]),
    ("ISO-8859-2", &[]),
    ("ISO-8859-3", &[]),
    ("ISO-8859-4", &[]),
    ("ISO-8859-5", &[]),
    ("ISO-8859-6", &[]),
    ("ISO-8859-7", &[]),
    ("ISO-8859-8", &[]),
    ("ISO-8859-8-I", &[]),
    ("ISO-8859-10", &[]),
    ("ISO-8859-13", &[]),
    ("ISO-8859-14", &[]),
    ("ISO-8859-15", &[]),
    ("ISO-8859-16", &[]),
    ("KOI8-R", &[]),
    ("KOI8-U", &[]),
    ("IBM866", &["cp866"]),
    ("macintosh", &["macroman"]),
    ("x-mac-cyrillic", &[]),
];

#[derive(Debug, Clone, Copy)]
pub struct InputEncoding(&'static Encoding);

impl InputEncoding {
    pub fn name(self) -> &'static str {
        self.0.name()
    }
}

impl fmt::Display for InputEncoding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for InputEncoding {
    type Err = String;

    fn from_str(label: &str) -> Result<Self, Self::Err> {
        for &(name, aliases) in ENCODINGS {
            if name.eq_ignore_ascii_case(label)
                || aliases
                    .iter()
                    .any(|alias| alias.eq_ignore_ascii_case(label))
            {
                return Ok(Self(Encoding::for_label(name.as_bytes()).unwrap()));
            }
        }
        Err(format!(
            "Unsupported encoding {label:?}; use --list-encodings"
        ))
    }
}

pub fn list_encodings() {
    println!("Encoding\tAliases (case-insensitive)");
    for (name, aliases) in ENCODINGS {
        println!("{name}\t{}", aliases.join(", "));
    }
    println!("\nWHATWG mappings: ascii/ISO-8859-1 use windows-1252; CP932 uses Shift_JIS.");
    println!("No automatic detection. UTF-32 and replacement encodings are unsupported.");
}

pub struct DecodedReader<R> {
    source: R,
    decoder: Decoder,
    encoding: InputEncoding,
    input: [u8; 8192],
    start: usize,
    end: usize,
    output: [u8; 32768],
    output_start: usize,
    output_end: usize,
    eof: bool,
    finished: bool,
    failed: bool,
}

impl<R: Read> DecodedReader<R> {
    pub fn new(mut source: R, encoding: InputEncoding) -> io::Result<Self> {
        let mut input = [0; 8192];
        let mut end = 0;
        // Read enough to distinguish UTF-32 BOMs from UTF-16, even on short reads.
        while end < 4 {
            match source.read(&mut input[end..4]) {
                Ok(0) => break,
                Ok(n) => end += n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        }
        let prefix = &input[..end];
        if prefix.starts_with(&[0xff, 0xfe, 0, 0]) || prefix.starts_with(&[0, 0, 0xfe, 0xff]) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "UTF-32 BOM is unsupported",
            ));
        }
        let bom = Encoding::for_bom(prefix);
        let start = match bom {
            Some((found, size)) if found == encoding.0 => size,
            Some((found, _)) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "BOM indicates {}, but specified encoding is {encoding}",
                        found.name()
                    ),
                ));
            }
            None => 0,
        };
        Ok(Self {
            source,
            decoder: encoding.0.new_decoder_without_bom_handling(),
            encoding,
            input,
            start,
            end,
            output: [0; 32768],
            output_start: 0,
            output_end: 0,
            eof: false,
            finished: false,
            failed: false,
        })
    }
}

impl<R: Read> Read for DecodedReader<R> {
    fn read(&mut self, destination: &mut [u8]) -> io::Result<usize> {
        if destination.is_empty() {
            return Ok(0);
        }
        loop {
            if self.failed {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Invalid or incomplete byte sequence for {}", self.encoding),
                ));
            }
            if self.output_start < self.output_end {
                let size = destination.len().min(self.output_end - self.output_start);
                destination[..size]
                    .copy_from_slice(&self.output[self.output_start..self.output_start + size]);
                self.output_start += size;
                return Ok(size);
            }
            if self.finished {
                return Ok(0);
            }
            if self.start == self.end && !self.eof {
                self.end = match self.source.read(&mut self.input) {
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    result => result?,
                };
                self.start = 0;
                self.eof = self.end == 0;
            }
            let (result, read, written) = self.decoder.decode_to_utf8_without_replacement(
                &self.input[self.start..self.end],
                &mut self.output,
                self.eof,
            );
            self.start += read;
            self.output_start = 0;
            self.output_end = written;
            match result {
                DecoderResult::Malformed(_, _) => self.failed = true,
                DecoderResult::InputEmpty if self.eof => self.finished = true,
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct OneByte<'a>(&'a [u8]);
    impl Read for OneByte<'_> {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            let n = out.len().min(self.0.len()).min(1);
            out[..n].copy_from_slice(&self.0[..n]);
            self.0 = &self.0[n..];
            Ok(n)
        }
    }

    fn decode(data: &[u8], encoding: &str) -> io::Result<String> {
        let mut reader = DecodedReader::new(OneByte(data), encoding.parse().unwrap())?;
        let mut text = String::new();
        reader.read_to_string(&mut text)?;
        Ok(text)
    }

    #[test]
    fn every_listed_name_and_alias_resolves() {
        for &(name, aliases) in ENCODINGS {
            let expected = name.parse::<InputEncoding>().unwrap().name();
            for alias in aliases {
                assert_eq!(
                    alias
                        .to_uppercase()
                        .parse::<InputEncoding>()
                        .unwrap()
                        .name(),
                    expected
                );
            }
        }
        for unsupported in ["auto", "utf-32", "replacement", "utf-16", "unknown"] {
            assert!(unsupported.parse::<InputEncoding>().is_err());
        }
    }

    #[test]
    fn decodes_multibyte_and_stateful_encodings_across_short_reads() {
        let text = "id,名前\n001,\"東京,大阪\n本社\"\n";
        for name in [
            "utf-8",
            "cp932",
            "euc-jp",
            "iso-2022-jp",
            "gb18030",
            "big5",
            "euc-kr",
        ] {
            let encoding: InputEncoding = name.parse().unwrap();
            let (data, _, errors) = encoding.0.encode(text);
            assert!(!errors, "{name}");
            assert_eq!(decode(&data, name).unwrap(), text);
        }
        for name in ["utf-16le", "utf-16be"] {
            let mut data = Vec::new();
            for unit in text.encode_utf16() {
                data.extend_from_slice(&if name.ends_with("le") {
                    unit.to_le_bytes()
                } else {
                    unit.to_be_bytes()
                });
            }
            assert_eq!(decode(&data, name).unwrap(), text);
        }
        let long = "😀日本語".repeat(10000);
        assert_eq!(decode(long.as_bytes(), "utf-8").unwrap(), long);
    }

    #[test]
    fn only_matching_boms_are_removed() {
        assert_eq!(decode(b"\xef\xbb\xbfhello", "utf-8").unwrap(), "hello");
        assert_eq!(decode(b"\xff\xfeA\0", "utf-16le").unwrap(), "A");
        assert_eq!(decode(b"\xfe\xff\0A", "utf-16be").unwrap(), "A");
        assert!(decode(b"\xff\xfeA\0", "utf-8").is_err());
        assert!(decode(b"\xef\xbb\xbfhello", "cp932").is_err());
        assert!(decode(b"\xff\xfe\0\0A\0\0\0", "utf-16le").is_err());
        assert_eq!(decode(b"", "utf-8").unwrap(), "");
        assert_eq!(decode(b"\xff\xfe", "utf-16le").unwrap(), "");
    }

    #[test]
    fn malformed_and_truncated_data_fail_without_replacement() {
        for (data, encoding) in [
            (b"a\xff".as_slice(), "utf-8"),
            (b"a\xe3\x81".as_slice(), "utf-8"),
            (b"a\x82".as_slice(), "cp932"),
            (b"a\0b".as_slice(), "utf-16le"),
            (b"\0\xd8".as_slice(), "utf-16le"),
        ] {
            assert!(decode(data, encoding).is_err(), "{encoding}");
        }
    }
}
