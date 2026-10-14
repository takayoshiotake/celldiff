use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Output};

fn cli(args: &[&std::ffi::OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_celldiff")).args(args).output().unwrap()
}
fn execute(before: &Path, after: &Path, output: &Path, force: bool) -> Output {
    let mut args = vec![before.as_os_str(), after.as_os_str(), std::ffi::OsStr::new("-o"), output.as_os_str()];
    if force { args.push(std::ffi::OsStr::new("--force")); }
    cli(&args)
}
fn xml(path: &Path, name: &str) -> String {
    let mut archive = zip::ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
    let mut value = String::new();
    archive.by_name(name).unwrap().read_to_string(&mut value).unwrap();
    roxmltree::Document::parse(&value).unwrap();
    value
}
fn cell_text(value: &str, address: &str) -> String {
    let doc = roxmltree::Document::parse(value).unwrap();
    let cell = doc.descendants().find(|n| n.has_tag_name("c") && n.attribute("r") == Some(address)).unwrap();
    cell.descendants().filter(|n| n.has_tag_name("t")).filter_map(|n| n.text()).collect()
}
fn fixture(old: &[u8], new: &[u8]) -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let before = dir.path().join("before file.csv");
    let after = dir.path().join("after file.csv");
    let output = dir.path().join("report.xlsx");
    fs::write(&before, old).unwrap();
    fs::write(&after, new).unwrap();
    (dir, before, after, output)
}

#[test]
fn partial_formats_work_in_constant_memory_and_preserve_original_values() {
    let (_dir, before, after, output) = fixture("001,東京本社,=1+1,\"a\nb\"\n".as_bytes(), "001,大阪本社,=2+2,\"a\nc\"\n".as_bytes());
    let result = execute(&before, &after, &output, false);
    assert_eq!(result.status.code(), Some(1), "{}", String::from_utf8_lossy(&result.stderr));
    assert!(String::from_utf8_lossy(&result.stdout).contains("changed_cells=3"));
    let sheet = xml(&output, "xl/worksheets/sheet1.xml");
    assert_eq!(cell_text(&sheet, "F2"), "東京本社");
    assert_eq!(cell_text(&sheet, "G2"), "大阪本社");
    assert_eq!(cell_text(&sheet, "F3"), "=1+1");
    assert_eq!(cell_text(&sheet, "G3"), "=2+2");
    assert_eq!(cell_text(&sheet, "F4"), "a\nb");
    assert!(sheet.contains("<strike"));
    assert!(sheet.contains("FFB91C1C"));
    assert!(sheet.contains("FF1D4ED8"));
    let doc = roxmltree::Document::parse(&sheet).unwrap();
    assert!(!doc.descendants().any(|n| n.has_tag_name("f")));
    assert!(doc.descendants().filter(|n| n.has_tag_name("c") && n.attribute("r").is_some_and(|r| r.starts_with('F') || r.starts_with('G'))).all(|n| n.attribute("t") == Some("inlineStr")));
    // A following row must not erase the previous row's rich text.
    assert!(doc.descendants().filter(|n| n.has_tag_name("row")).count() == 4);
}

#[test]
fn empty_and_missing_have_different_presence_flags() {
    let (_dir, before, after, output) = fixture(b"a,\n", b"a\nnew,\n");
    let result = execute(&before, &after, &output, false);
    assert_eq!(result.status.code(), Some(1));
    let sheet = xml(&output, "xl/worksheets/sheet1.xml");
    let doc = roxmltree::Document::parse(&sheet).unwrap();
    let flag = |addr| {
        doc.descendants().find(|n| n.has_tag_name("c") && n.attribute("r") == Some(addr)).unwrap()
            .descendants().find(|n| n.has_tag_name("v")).unwrap().text().unwrap()
    };
    assert_eq!(cell_text(&sheet, "C2"), "Removed");
    assert_eq!(flag("D2"), "1");
    assert_eq!(flag("E2"), "0");
    assert_eq!(cell_text(&sheet, "C4"), "Added");
    assert_eq!(flag("D4"), "0");
    assert_eq!(flag("E4"), "1");
}

#[test]
fn identical_and_empty_inputs_still_generate_reports() {
    for input in [b"a,b\n".as_slice(), b"".as_slice()] {
        let (_dir, before, after, output) = fixture(input, input);
        let result = execute(&before, &after, &output, false);
        assert_eq!(result.status.code(), Some(0));
        assert!(String::from_utf8_lossy(&result.stdout).contains("changed_cells=0"));
        let sheet = xml(&output, "xl/worksheets/sheet1.xml");
        let doc = roxmltree::Document::parse(&sheet).unwrap();
        assert_eq!(doc.descendants().filter(|n| n.has_tag_name("row")).count(), 1);
        xml(&output, "xl/worksheets/sheet2.xml");
    }
}

#[test]
fn complete_value_changes_keep_leading_zeros_and_formula_like_text() {
    let (_dir, before, after, output) = fixture(b"001,=1\n", b"002,+2\n");
    assert_eq!(execute(&before, &after, &output, false).status.code(), Some(1));
    let sheet = xml(&output, "xl/worksheets/sheet1.xml");
    assert_eq!(cell_text(&sheet, "F2"), "001");
    assert_eq!(cell_text(&sheet, "G2"), "002");
    assert_eq!(cell_text(&sheet, "F3"), "=1");
    assert_eq!(cell_text(&sheet, "G3"), "+2");
}

#[test]
fn existing_output_is_protected_and_force_replaces_only_on_success() {
    let (_dir, before, after, output) = fixture(b"a\n", b"b\n");
    fs::write(&output, "keep").unwrap();
    assert_eq!(execute(&before, &after, &output, false).status.code(), Some(2));
    assert_eq!(fs::read_to_string(&output).unwrap(), "keep");
    assert_eq!(execute(&before, &after, &output, true).status.code(), Some(1));
    xml(&output, "xl/worksheets/sheet1.xml");
    fs::write(&before, b"\xff\n").unwrap();
    let saved = fs::read(&output).unwrap();
    assert_eq!(execute(&before, &after, &output, true).status.code(), Some(2));
    assert_eq!(fs::read(&output).unwrap(), saved);
}

#[test]
fn invalid_utf8_and_oversize_cell_publish_no_report() {
    for old in [vec![0xff, b'\n'], format!("{}\n", "a".repeat(32768)).into_bytes()] {
        let (dir, before, after, output) = fixture(&old, b"b\n");
        let result = execute(&before, &after, &output, false);
        assert_eq!(result.status.code(), Some(2));
        assert!(!output.exists());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    }
}

#[test]
fn input_aliases_and_bad_output_paths_are_rejected() {
    let (dir, before, after, output) = fixture(b"a\n", b"b\n");
    let hardlink = dir.path().join("alias.xlsx");
    fs::hard_link(&before, &hardlink).unwrap();
    assert_eq!(execute(&before, &after, &hardlink, true).status.code(), Some(2));
    assert_eq!(fs::read(&before).unwrap(), b"a\n");
    assert_eq!(execute(&before, &after, &output.with_extension("csv"), false).status.code(), Some(2));
    assert_eq!(execute(&before, &after, &dir.path().join("missing/report.xlsx"), false).status.code(), Some(2));
    assert_eq!(execute(&dir.path().join("absent.csv"), &after, &output, false).status.code(), Some(2));
}

#[test]
fn help_version_and_argument_errors_are_consistent() {
    assert_eq!(cli(&[std::ffi::OsStr::new("--help")]).status.code(), Some(0));
    assert_eq!(cli(&[std::ffi::OsStr::new("--version")]).status.code(), Some(0));
    assert_eq!(cli(&[]).status.code(), Some(2));
    assert_eq!(cli(&[std::ffi::OsStr::new("--unknown")]).status.code(), Some(2));
}
