use std::fs;
use std::path::Path;
use std::process::{Command, Output, Stdio};

fn fixture(left: &[u8], right: &[u8]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("left.csv"), left).unwrap();
    fs::write(dir.path().join("right.csv"), right).unwrap();
    dir
}

fn run(dir: &Path, options: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_celldiff"))
        .current_dir(dir)
        .args(["left.csv", "right.csv"])
        .args(options)
        .output()
        .unwrap()
}

fn output(result: Output, code: i32) -> String {
    assert_eq!(
        result.status.code(),
        Some(code),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).unwrap()
}

#[test]
fn default_and_oneline_snapshots_only_include_changed_cells() {
    let dir = fixture(
        "id,name\n001,東京本社\n002,\n004,x\n".as_bytes(),
        "id,name\n001,大阪本社\n002\n004,x,\n".as_bytes(),
    );
    let text = output(run(dir.path(), &["--text"]), 1);
    assert_eq!(
        text,
        "B2:\n  - \"東京本社\"\n  + \"大阪本社\"\n\nB3:\n  - \"\"\n  + <missing>\n\nC4:\n  - <missing>\n  + \"\"\n\nChanged cells: 3 (modified=1, added=1, removed=1)\n"
    );
    let line = output(run(dir.path(), &["--text", "--oneline"]), 1);
    assert_eq!(
        line,
        "B2: \"東京本社\" -> \"大阪本社\"\nB3: \"\" -> <missing>\nC4: <missing> -> \"\"\n\nChanged cells: 3 (modified=1, added=1, removed=1)\n"
    );
    assert!(!dir.path().join("diff.xlsx").exists());
}

#[test]
fn address_styles_work_with_both_layouts_and_logical_record_numbers() {
    let dir = fixture(b"\"a\nb\",x\nsame,y\n", b"\"a\nb\",z\nsame,w\n");
    let text = output(run(dir.path(), &["--text", "--address", "rc"]), 1);
    assert!(text.starts_with("row=1 column=2:\n  - \"x\"\n  + \"z\"\n\nrow=2 column=2:"));
    let line = output(run(dir.path(), &["--text", "--oneline", "--address=rc"]), 1);
    assert!(line.starts_with("row=1 column=2: \"x\" -> \"z\"\nrow=2 column=2: \"y\" -> \"w\"\n"));
    let a1 = output(run(dir.path(), &["--text", "--address", "a1"]), 1);
    assert!(a1.starts_with("B1:\n"));
    assert!(a1.contains("\nB2:\n"));
}

#[test]
fn controls_quotes_backslashes_and_missing_are_unambiguous() {
    let mut writer = csv::Writer::from_writer(Vec::new());
    writer
        .write_record(["same", "", "<missing>", "a\nb\t\"\\\u{1b}\0"])
        .unwrap();
    let left = writer.into_inner().unwrap();
    let dir = fixture(&left, b"same,x,,changed,\n");
    let text = output(run(dir.path(), &["--text", "--oneline"]), 1);
    assert_eq!(
        text,
        "B1: \"\" -> \"x\"\nC1: \"<missing>\" -> \"\"\nD1: \"a\\nb\\t\\\"\\\\\\u{1b}\\0\" -> \"changed\"\nE1: <missing> -> \"\"\n\nChanged cells: 4 (modified=3, added=1, removed=0)\n"
    );
    assert!(
        !text.contains('\r')
            && !text.contains('\t')
            && !text.contains('\u{1b}')
            && !text.contains('\0')
    );
}

#[test]
fn no_differences_and_existing_default_output_are_untouched() {
    for input in [b"".as_slice(), b"001,same\n"] {
        let dir = fixture(input, input);
        fs::write(dir.path().join("diff.xlsx"), "keep").unwrap();
        for options in [vec!["--text"], vec!["--text", "--oneline"]] {
            assert_eq!(output(run(dir.path(), &options), 0), "No differences.\n");
            assert_eq!(
                fs::read_to_string(dir.path().join("diff.xlsx")).unwrap(),
                "keep"
            );
            assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 3);
        }
    }
    let dir = fixture(b"a\n", b"b\n");
    // Even an unusable default output path is irrelevant to --text.
    fs::create_dir(dir.path().join("diff.xlsx")).unwrap();
    output(run(dir.path(), &["--text"]), 1);
    assert!(dir.path().join("diff.xlsx").is_dir());
}

#[test]
fn text_option_dependencies_and_explicit_conflicts_are_argument_errors() {
    let dir = fixture(b"a\n", b"b\n");
    let cases: &[&[&str]] = &[
        &["--oneline"],
        &["--address", "rc"],
        &["--address", "a1"],
        &["--text", "--address", "unknown"],
        &["--text", "-o", "selected.xlsx"],
        &["--text", "--output=diff.xlsx"],
        &["--text", "--force"],
        &["--text", "--list-encodings"],
        &["--oneline", "--list-encodings"],
        &["--address=a1", "--list-encodings"],
        &["--format", "text"],
        &["--summary"],
    ];
    for options in cases {
        let result = run(dir.path(), options);
        assert_eq!(result.status.code(), Some(2), "{options:?}");
        assert!(result.stdout.is_empty(), "{options:?}");
        assert!(String::from_utf8_lossy(&result.stderr).contains("error:"));
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    }
    let help = Command::new(env!("CARGO_BIN_EXE_celldiff"))
        .arg("--help")
        .output()
        .unwrap();
    let help = output(help, 0);
    assert!(help.contains("--address") && help.contains("a1") && help.contains("rc"));
    assert!(help.contains("[default: a1]"));
}

#[test]
fn text_output_honors_independent_encodings() {
    let (left, _, errors) = encoding_rs::SHIFT_JIS.encode("001,東京本社\n");
    assert!(!errors);
    let dir = fixture(&left, "001,大阪本社\n".as_bytes());
    let text = output(
        run(
            dir.path(),
            &["--text", "--oneline", "--el", "cp932", "--er", "utf-8"],
        ),
        1,
    );
    assert_eq!(
        text,
        "B1: \"東京本社\" -> \"大阪本社\"\n\nChanged cells: 1 (modified=1, added=0, removed=0)\n"
    );
}

#[test]
fn late_input_failure_does_not_print_successful_totals_or_create_files() {
    let mut left = b"a\n".repeat(6000);
    left.extend_from_slice(b"\xff\n");
    let mut right = b"b\n".to_vec();
    right.extend_from_slice(&b"a\n".repeat(6000));
    let dir = fixture(&left, &right);
    let result = run(dir.path(), &["--text", "--oneline"]);
    assert_eq!(result.status.code(), Some(2));
    let text = String::from_utf8(result.stdout).unwrap();
    assert!(text.starts_with("A1: \"a\" -> \"b\"\n"));
    assert!(!text.contains("Changed cells:") && !text.contains("No differences."));
    assert!(String::from_utf8_lossy(&result.stderr).contains("left.csv"));
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
}

#[test]
fn text_does_not_apply_excel_column_and_text_limits() {
    let left = format!("{}\n", ",".repeat(16384));
    let right = format!("{}x\n", ",".repeat(16384));
    let dir = fixture(left.as_bytes(), right.as_bytes());
    assert!(
        output(run(dir.path(), &["--text", "--oneline"]), 1).starts_with("XFE1: \"\" -> \"x\"\n")
    );
    let left = format!("{}\n", "a".repeat(32768));
    let dir = fixture(left.as_bytes(), b"b\n");
    assert!(output(run(dir.path(), &["--text", "--oneline"]), 1).contains("modified=1"));
}

#[test]
fn closed_stdout_pipe_is_reported_as_an_error() {
    let dir = fixture(&b"a\n".repeat(20000), &b"b\n".repeat(20000));
    let mut child = Command::new(env!("CARGO_BIN_EXE_celldiff"))
        .current_dir(dir.path())
        .args(["left.csv", "right.csv", "--text", "--oneline"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    let result = child.wait_with_output().unwrap();
    assert_eq!(result.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&result.stderr).contains("celldiff:"));
    assert!(!dir.path().join("diff.xlsx").exists());
}

#[test]
fn text_does_not_apply_excel_row_limit() {
    let mut left = b"a\n".repeat(1048576);
    left.extend_from_slice(b"x\n");
    let mut right = b"a\n".repeat(1048576);
    right.extend_from_slice(b"y\n");
    let dir = fixture(&left, &right);
    let text = output(run(dir.path(), &["--text", "--oneline"]), 1);
    assert_eq!(
        text,
        "A1048577: \"x\" -> \"y\"\n\nChanged cells: 1 (modified=1, added=0, removed=0)\n"
    );
}
