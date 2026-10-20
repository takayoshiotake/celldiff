use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Output};

fn fixture(left: &[u8], right: &[u8]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("left.csv"), left).unwrap();
    fs::write(dir.path().join("right.csv"), right).unwrap();
    dir
}
fn run(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_celldiff"))
        .current_dir(dir)
        .args(["left.csv", "right.csv"])
        .args(args)
        .output()
        .unwrap()
}
fn stdout(result: Output, code: i32) -> String {
    assert_eq!(
        result.status.code(),
        Some(code),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).unwrap()
}
fn xml(path: &Path, name: &str) -> String {
    let mut archive = zip::ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
    let mut text = String::new();
    archive
        .by_name(name)
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    text
}
fn cell(sheet: &str, address: &str) -> String {
    let document = roxmltree::Document::parse(sheet).unwrap();
    document
        .descendants()
        .find(|n| n.has_tag_name("c") && n.attribute("r") == Some(address))
        .unwrap()
        .descendants()
        .filter(|n| n.has_tag_name("t"))
        .filter_map(|n| n.text())
        .collect()
}

#[test]
fn quotes_are_ignored_by_default_and_compared_only_when_requested() {
    let dir = fixture(b"1\n", b"\"1\"\r\n");
    assert_eq!(stdout(run(dir.path(), &["--text"]), 0), "No differences.\n");
    assert_eq!(
        stdout(
            run(dir.path(), &["--compare-quotes", "--text", "--oneline"]),
            1
        ),
        "A1: \"1\" [unquoted] -> \"1\" [quoted]\n\nChanged cells: 1 (modified=1, added=0, removed=0)\n"
    );
    let reversed = fixture(b"\"1\"\n", b"1\n");
    assert!(
        stdout(run(reversed.path(), &["--compare-quotes", "--text"]), 1)
            .starts_with("A1:\n  - \"1\" [quoted]\n  + \"1\" [unquoted]\n")
    );
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
}

#[test]
fn quoted_empty_fields_are_distinct_from_unquoted_empty_and_missing() {
    let dir = fixture(b"a,\n", b"a,\"\"\n");
    assert_eq!(stdout(run(dir.path(), &["--text"]), 0), "No differences.\n");
    assert_eq!(
        stdout(
            run(
                dir.path(),
                &["--compare-quotes", "--text", "--oneline", "--address", "rc"]
            ),
            1
        ),
        "row=1 column=2: \"\" [unquoted] -> \"\" [quoted]\n\nChanged cells: 1 (modified=1, added=0, removed=0)\n"
    );
    let dir = fixture(b"a,\"\"\n\"x\",b\n", b"a\n\"x\",b,\"new\"\n");
    assert_eq!(
        stdout(
            run(dir.path(), &["--compare-quotes", "--text", "--oneline"]),
            1
        ),
        "B1: \"\" [quoted] -> <missing>\nC2: <missing> -> \"new\" [quoted]\n\nChanged cells: 2 (modified=0, added=1, removed=1)\n"
    );
}

#[test]
fn value_and_quote_changes_count_as_one_modified_cell() {
    let dir = fixture(b"1\n", b"\"2\"\n");
    assert_eq!(
        stdout(
            run(dir.path(), &["--compare-quotes", "--text", "--oneline"]),
            1
        ),
        "A1: \"1\" [unquoted] -> \"2\" [quoted]\n\nChanged cells: 1 (modified=1, added=0, removed=0)\n"
    );
    let dir = fixture(b"a\"b\n", b"\"a\"\"b\"\n");
    assert_eq!(stdout(run(dir.path(), &["--text"]), 0), "No differences.\n");
    assert!(
        stdout(
            run(dir.path(), &["--compare-quotes", "--text", "--oneline"]),
            1
        )
        .contains("modified=1")
    );
}

#[test]
fn excel_preserves_coordinates_and_highlights_quote_only_changes() {
    let dir = fixture(b"1,\"a\"\"b\"\n", b"\"1\",\"a\"\"b\"\n");
    let result = run(dir.path(), &["--compare-quotes"]);
    assert!(stdout(result, 1).contains("changed_cells=1"));
    let output = dir.path().join("diff.xlsx");
    let sheet = xml(&output, "xl/worksheets/sheet1.xml");
    assert_eq!(cell(&sheet, "A1"), "\"1\"");
    assert_eq!(cell(&sheet, "B1"), "\"a\"\"b\"");
    assert!(sheet.contains("A1:B1") && sheet.contains("FF1D4ED8"));
    assert!(xml(&output, "xl/sharedStrings.xml").contains("Enabled; display CSV field quoting"));
    assert!(xml(&output, "xl/styles.xml").contains("FFFEF9C3"));
    let default = stdout(run(dir.path(), &["-o", "default.xlsx"]), 0);
    assert!(default.contains("changed_cells=0"));
    let ordinary = xml(&dir.path().join("default.xlsx"), "xl/worksheets/sheet1.xml");
    assert_eq!(cell(&ordinary, "A1"), "1");
    assert_eq!(cell(&ordinary, "B1"), "a\"b");
    let reverse = fixture(b"\"1\"\n", b"1\n");
    stdout(run(reverse.path(), &["--compare-quotes"]), 1);
    let removed = xml(
        &reverse.path().join("diff.xlsx"),
        "xl/worksheets/sheet1.xml",
    );
    assert!(removed.contains("FFB91C1C") && removed.contains("<strike"));
}

#[test]
fn encodings_bom_and_record_terminators_are_not_raw_differences() {
    let (left, _, errors) = encoding_rs::SHIFT_JIS.encode("\"東京\",\"a,b\",\"a\"\"b\"\r\n\n");
    assert!(!errors);
    let right = "\"東京\",\"a,b\",\"a\"\"b\"\n";
    for encoding in ["utf-8", "utf-16le", "utf-16be"] {
        let mut data = Vec::new();
        if encoding == "utf-8" {
            data.extend_from_slice(b"\xef\xbb\xbf");
            data.extend_from_slice(right.as_bytes());
        } else {
            data.extend_from_slice(if encoding.ends_with("le") {
                b"\xff\xfe"
            } else {
                b"\xfe\xff"
            });
            for unit in right.encode_utf16() {
                data.extend_from_slice(&if encoding.ends_with("le") {
                    unit.to_le_bytes()
                } else {
                    unit.to_be_bytes()
                });
            }
        }
        let dir = fixture(&left, &data);
        assert_eq!(
            stdout(
                run(
                    dir.path(),
                    &[
                        "--compare-quotes",
                        "--text",
                        "--el",
                        "cp932",
                        "--er",
                        encoding
                    ]
                ),
                0
            ),
            "No differences.\n"
        );
    }
    let (left, _, _) = encoding_rs::SHIFT_JIS.encode("001,東京\n");
    let dir = fixture(&left, "001,\"東京\"\n".as_bytes());
    assert!(
        stdout(
            run(
                dir.path(),
                &["--compare-quotes", "--text", "--oneline", "--el", "cp932"]
            ),
            1
        )
        .contains("B1: \"東京\" [unquoted] -> \"東京\" [quoted]")
    );
}

#[test]
fn quoted_multiline_and_buffer_boundaries_keep_correct_record_numbers() {
    let left = format!("\"{}\",a\nlast,b\n", "x\n".repeat(10000));
    let right = format!("\"{}\",\"a\"\nlast,b\n", "x\n".repeat(10000));
    let dir = fixture(left.as_bytes(), right.as_bytes());
    assert_eq!(
        stdout(
            run(
                dir.path(),
                &["--compare-quotes", "--text", "--oneline", "--address", "rc"]
            ),
            1
        ),
        "row=1 column=2: \"a\" [unquoted] -> \"a\" [quoted]\n\nChanged cells: 1 (modified=1, added=0, removed=0)\n"
    );
}

#[test]
fn errors_and_expanded_excel_text_limits_preserve_existing_output() {
    let value = "a".repeat(32767);
    let dir = fixture(
        format!("{value}\n").as_bytes(),
        format!("\"{value}\"\n").as_bytes(),
    );
    let output = dir.path().join("diff.xlsx");
    fs::write(&output, "keep").unwrap();
    let result = run(dir.path(), &["--compare-quotes", "--force"]);
    assert_eq!(result.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&result.stderr).contains("32767"));
    assert_eq!(fs::read_to_string(&output).unwrap(), "keep");
    assert!(
        stdout(
            run(dir.path(), &["--compare-quotes", "--text", "--oneline"]),
            1
        )
        .contains("modified=1")
    );
    fs::write(dir.path().join("right.csv"), b"\"bad\xff\"\n").unwrap();
    assert_eq!(
        run(dir.path(), &["--compare-quotes", "--force"])
            .status
            .code(),
        Some(2)
    );
    assert_eq!(fs::read_to_string(&output).unwrap(), "keep");
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 3);
}

#[test]
fn quote_option_is_in_help_and_conflicts_with_encoding_listing() {
    let help = Command::new(env!("CARGO_BIN_EXE_celldiff"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(stdout(help, 0).contains("--compare-quotes"));
    let result = Command::new(env!("CARGO_BIN_EXE_celldiff"))
        .args(["--compare-quotes", "--list-encodings"])
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
    assert!(result.stdout.is_empty());
}
