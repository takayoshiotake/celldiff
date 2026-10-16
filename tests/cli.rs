use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Output};

fn cli(args: &[&std::ffi::OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_celldiff"))
        .args(args)
        .output()
        .unwrap()
}
fn execute(left: &Path, right: &Path, output: &Path, force: bool) -> Output {
    let mut args = vec![
        left.as_os_str(),
        right.as_os_str(),
        std::ffi::OsStr::new("-o"),
        output.as_os_str(),
    ];
    if force {
        args.push(std::ffi::OsStr::new("--force"));
    }
    cli(&args)
}
fn xml(path: &Path, name: &str) -> String {
    let mut archive = zip::ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
    let mut value = String::new();
    archive
        .by_name(name)
        .unwrap()
        .read_to_string(&mut value)
        .unwrap();
    roxmltree::Document::parse(&value).unwrap();
    value
}
fn cell_text(value: &str, address: &str) -> String {
    let doc = roxmltree::Document::parse(value).unwrap();
    let cell = doc
        .descendants()
        .find(|n| n.has_tag_name("c") && n.attribute("r") == Some(address))
        .unwrap();
    cell.descendants()
        .filter(|n| n.has_tag_name("t"))
        .filter_map(|n| n.text())
        .collect()
}
fn workbook_cell_text(path: &Path, sheet: &str, address: &str) -> String {
    let value = xml(path, sheet);
    let doc = roxmltree::Document::parse(&value).unwrap();
    let cell = doc
        .descendants()
        .find(|n| n.has_tag_name("c") && n.attribute("r") == Some(address))
        .unwrap();
    if cell.attribute("t") != Some("s") {
        return cell_text(&value, address);
    }
    let index: usize = cell
        .children()
        .find(|n| n.has_tag_name("v"))
        .unwrap()
        .text()
        .unwrap()
        .parse()
        .unwrap();
    let strings = xml(path, "xl/sharedStrings.xml");
    let strings = roxmltree::Document::parse(&strings).unwrap();
    strings
        .descendants()
        .filter(|n| n.has_tag_name("si"))
        .nth(index)
        .unwrap()
        .descendants()
        .filter(|n| n.has_tag_name("t"))
        .filter_map(|n| n.text())
        .collect()
}

fn fixture(
    old: &[u8],
    new: &[u8],
) -> (
    tempfile::TempDir,
    std::path::PathBuf,
    std::path::PathBuf,
    std::path::PathBuf,
) {
    let dir = tempfile::tempdir().unwrap();
    let left = dir.path().join("left file.csv");
    let right = dir.path().join("right file.csv");
    let output = dir.path().join("report.xlsx");
    fs::write(&left, old).unwrap();
    fs::write(&right, new).unwrap();
    (dir, left, right, output)
}

#[test]
fn original_coordinates_and_partial_formats_are_preserved() {
    let (_dir, left, right, output) = fixture(
        "id,name,note\n001,東京本社,\"a\nb\"\n002,same,=1+1\n".as_bytes(),
        "id,name,note\n001,大阪本社,\"a\nc\"\n002,same,=2+2\n".as_bytes(),
    );
    let result = execute(&left, &right, &output, false);
    assert_eq!(
        result.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(String::from_utf8_lossy(&result.stdout).contains("changed_cells=3"));
    let sheet = xml(&output, "xl/worksheets/sheet1.xml");
    assert_eq!(cell_text(&sheet, "A1"), "id");
    assert_eq!(cell_text(&sheet, "B1"), "name");
    assert_eq!(cell_text(&sheet, "A2"), "001");
    assert_eq!(cell_text(&sheet, "B2"), "東京大阪本社");
    assert_eq!(cell_text(&sheet, "C2"), "a\nbc");
    assert_eq!(cell_text(&sheet, "B3"), "same");
    assert_eq!(cell_text(&sheet, "C3"), "=12+12");
    assert!(sheet.contains("<strike"));
    assert!(sheet.contains("FFB91C1C") && sheet.contains("FF1D4ED8"));
    let doc = roxmltree::Document::parse(&sheet).unwrap();
    assert!(!doc.descendants().any(|n| n.has_tag_name("f")));
    assert_eq!(
        doc.descendants().filter(|n| n.has_tag_name("row")).count(),
        3
    );
    assert_eq!(
        doc.descendants()
            .find(|n| n.has_tag_name("dimension"))
            .unwrap()
            .attribute("ref"),
        Some("A1:C3")
    );
}

#[test]
fn empty_added_and_removed_cells_have_distinct_fills_without_extra_columns() {
    let (_dir, left, right, output) = fixture(b"a,\n", b"a\nnew,\n");
    assert_eq!(
        execute(&left, &right, &output, false).status.code(),
        Some(1)
    );
    let sheet = xml(&output, "xl/worksheets/sheet1.xml");
    let styles = xml(&output, "xl/styles.xml");
    let document = roxmltree::Document::parse(&sheet).unwrap();
    let style_document = roxmltree::Document::parse(&styles).unwrap();
    let fill_color = |address| {
        let cell = document
            .descendants()
            .find(|n| n.has_tag_name("c") && n.attribute("r") == Some(address))
            .unwrap();
        let index: usize = cell.attribute("s").unwrap().parse().unwrap();
        let xfs = style_document
            .descendants()
            .find(|n| n.has_tag_name("cellXfs"))
            .unwrap();
        let xf = xfs
            .children()
            .filter(|n| n.is_element())
            .nth(index)
            .unwrap();
        let fill_id: usize = xf.attribute("fillId").unwrap().parse().unwrap();
        let fills = style_document
            .descendants()
            .find(|n| n.has_tag_name("fills"))
            .unwrap();
        fills
            .children()
            .filter(|n| n.is_element())
            .nth(fill_id)
            .unwrap()
            .descendants()
            .find(|n| n.has_tag_name("fgColor"))
            .unwrap()
            .attribute("rgb")
            .unwrap()
            .to_string()
    };
    assert_eq!(cell_text(&sheet, "A1"), "a");
    assert_eq!(cell_text(&sheet, "A2"), "new");
    assert_eq!(fill_color("B1"), "FFFEE2E2");
    assert_eq!(fill_color("B2"), "FFDBEAFE");
    assert_eq!(
        document
            .descendants()
            .find(|n| n.has_tag_name("dimension"))
            .unwrap()
            .attribute("ref"),
        Some("A1:B2")
    );
}

#[test]
fn identical_and_empty_inputs_preserve_the_full_grid() {
    for input in [b"a,b\n".as_slice(), b"".as_slice()] {
        let (_dir, left, right, output) = fixture(input, input);
        let result = execute(&left, &right, &output, false);
        assert_eq!(result.status.code(), Some(0));
        let sheet = xml(&output, "xl/worksheets/sheet1.xml");
        let doc = roxmltree::Document::parse(&sheet).unwrap();
        assert_eq!(
            doc.descendants().filter(|n| n.has_tag_name("row")).count(),
            usize::from(!input.is_empty())
        );
        if !input.is_empty() {
            assert_eq!(cell_text(&sheet, "A1"), "a");
            assert_eq!(cell_text(&sheet, "B1"), "b");
        }
        xml(&output, "xl/worksheets/sheet2.xml");
    }
}

#[test]
fn strings_keep_leading_zeros_and_formula_like_values() {
    let (_dir, left, right, output) = fixture(b"001,=1\n", b"001,+2\n");
    assert_eq!(
        execute(&left, &right, &output, false).status.code(),
        Some(1)
    );
    let sheet = xml(&output, "xl/worksheets/sheet1.xml");
    assert_eq!(cell_text(&sheet, "A1"), "001");
    assert_eq!(cell_text(&sheet, "B1"), "=1+2");
    let doc = roxmltree::Document::parse(&sheet).unwrap();
    assert!(!doc.descendants().any(|n| n.has_tag_name("f")));
}

#[test]
fn unchanged_rows_are_not_collapsed_when_only_a_later_cell_changes() {
    let (_dir, left, right, output) = fixture(b"a,b\nc,d\ne,f\n", b"a,b\nc,d\ne,g\n");
    assert_eq!(
        execute(&left, &right, &output, false).status.code(),
        Some(1)
    );
    let sheet = xml(&output, "xl/worksheets/sheet1.xml");
    assert_eq!(cell_text(&sheet, "B1"), "b");
    assert_eq!(cell_text(&sheet, "B2"), "d");
    assert_eq!(cell_text(&sheet, "B3"), "fg");
}

#[test]
fn combined_replacement_limit_does_not_truncate_or_publish() {
    let old = format!("{}\n", "a".repeat(20000));
    let new = format!("{}\n", "b".repeat(20000));
    let (_dir, left, right, output) = fixture(old.as_bytes(), new.as_bytes());
    assert_eq!(
        execute(&left, &right, &output, false).status.code(),
        Some(2)
    );
    assert!(!output.exists());
}

#[test]
fn too_many_input_columns_are_rejected_even_when_identical() {
    let input = vec!["a"; 16385].join(",");
    let (_dir, left, right, output) = fixture(input.as_bytes(), input.as_bytes());
    assert_eq!(
        execute(&left, &right, &output, false).status.code(),
        Some(2)
    );
    assert!(!output.exists());
}

#[test]
fn existing_output_is_protected_and_force_replaces_only_on_success() {
    let (_dir, left, right, output) = fixture(b"a\n", b"b\n");
    fs::write(&output, "keep").unwrap();
    assert_eq!(
        execute(&left, &right, &output, false).status.code(),
        Some(2)
    );
    assert_eq!(fs::read_to_string(&output).unwrap(), "keep");
    assert_eq!(execute(&left, &right, &output, true).status.code(), Some(1));
    xml(&output, "xl/worksheets/sheet1.xml");
    fs::write(&left, b"\xff\n").unwrap();
    let saved = fs::read(&output).unwrap();
    assert_eq!(execute(&left, &right, &output, true).status.code(), Some(2));
    assert_eq!(fs::read(&output).unwrap(), saved);
}

#[test]
fn invalid_utf8_and_oversize_cell_publish_no_report() {
    for old in [
        vec![0xff, b'\n'],
        format!("{}\n", "a".repeat(32768)).into_bytes(),
    ] {
        let (dir, left, right, output) = fixture(&old, b"b\n");
        let result = execute(&left, &right, &output, false);
        assert_eq!(result.status.code(), Some(2));
        assert!(!output.exists());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    }
}

#[test]
fn input_aliases_and_bad_output_paths_are_rejected() {
    let (dir, left, right, output) = fixture(b"a\n", b"b\n");
    let hardlink = dir.path().join("alias.xlsx");
    fs::hard_link(&left, &hardlink).unwrap();
    assert_eq!(
        execute(&left, &right, &hardlink, true).status.code(),
        Some(2)
    );
    assert_eq!(fs::read(&left).unwrap(), b"a\n");
    assert_eq!(
        execute(&left, &right, &output.with_extension("csv"), false)
            .status
            .code(),
        Some(2)
    );
    assert_eq!(
        execute(
            &left,
            &right,
            &dir.path().join("missing/report.xlsx"),
            false
        )
        .status
        .code(),
        Some(2)
    );
    assert_eq!(
        execute(&dir.path().join("absent.csv"), &right, &output, false)
            .status
            .code(),
        Some(2)
    );
}

#[test]
fn help_version_and_argument_errors_are_consistent() {
    assert_eq!(
        cli(&[std::ffi::OsStr::new("--help")]).status.code(),
        Some(0)
    );
    assert_eq!(
        cli(&[std::ffi::OsStr::new("--version")]).status.code(),
        Some(0)
    );
    assert_eq!(cli(&[]).status.code(), Some(2));
    assert_eq!(
        cli(&[std::ffi::OsStr::new("--unknown")]).status.code(),
        Some(2)
    );
}

fn execute_encoded(left: &Path, right: &Path, output: &Path, options: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_celldiff"))
        .arg(left)
        .arg(right)
        .arg("-o")
        .arg(output)
        .args(options)
        .output()
        .unwrap()
}

#[test]
fn explicit_encodings_compare_same_values_and_record_selected_names() {
    let text = "id,name,note\n001,東京,\"a,b\nc\"\n002,,\n";
    let (encoded, _, errors) = encoding_rs::SHIFT_JIS.encode(text);
    assert!(!errors);
    for options in [
        vec!["--encoding-left", "cp932"],
        vec!["--el", "WINDOWS-31J", "--er", "utf8"],
    ] {
        let (_dir, left, right, output) = fixture(&encoded, text.as_bytes());
        let result = execute_encoded(&left, &right, &output, &options);
        assert_eq!(
            result.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let sheet = xml(&output, "xl/worksheets/sheet1.xml");
        assert_eq!(cell_text(&sheet, "A2"), "001");
        assert_eq!(cell_text(&sheet, "B2"), "東京");
        assert_eq!(cell_text(&sheet, "C2"), "a,b\nc");
        assert_eq!(
            workbook_cell_text(&output, "xl/worksheets/sheet2.xml", "A3"),
            "Left encoding"
        );
        assert_eq!(
            workbook_cell_text(&output, "xl/worksheets/sheet2.xml", "B3"),
            "Shift_JIS"
        );
        assert_eq!(
            workbook_cell_text(&output, "xl/worksheets/sheet2.xml", "A4"),
            "Right encoding"
        );
        assert_eq!(
            workbook_cell_text(&output, "xl/worksheets/sheet2.xml", "B4"),
            "UTF-8"
        );
    }
    let (_dir, left, right, output) = fixture(text.as_bytes(), &encoded);
    let result = execute_encoded(&left, &right, &output, &["--encoding-right", "cp932"]);
    assert_eq!(result.status.code(), Some(0));
    let (encoded, _, _) = encoding_rs::SHIFT_JIS.encode("001,東京本社\n");
    let (_dir, left, right, output) = fixture(&encoded, "001,大阪本社\n".as_bytes());
    let result = execute_encoded(&left, &right, &output, &["--el", "cp932"]);
    assert_eq!(result.status.code(), Some(1));
    assert_eq!(
        cell_text(&xml(&output, "xl/worksheets/sheet1.xml"), "B1"),
        "東京大阪本社"
    );
}

#[test]
fn utf16_with_and_without_bom_matches_utf8() {
    let text = "001,東京😀\n";
    for (encoding, big_endian, bom) in [
        ("utf-16le", false, false),
        ("utf-16le", false, true),
        ("utf-16be", true, false),
        ("utf-16be", true, true),
    ] {
        let mut data = Vec::new();
        if bom {
            data.extend_from_slice(if big_endian { b"\xfe\xff" } else { b"\xff\xfe" });
        }
        for unit in text.encode_utf16() {
            data.extend_from_slice(&if big_endian {
                unit.to_be_bytes()
            } else {
                unit.to_le_bytes()
            });
        }
        let (_dir, left, right, output) = fixture(text.as_bytes(), &data);
        let result = execute_encoded(&left, &right, &output, &["--er", encoding]);
        assert_eq!(
            result.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
}

#[test]
fn encoding_errors_preserve_output_and_identify_input() {
    for (data, encoding) in [
        (b"a\x82".as_slice(), "cp932"),
        (b"a\0b".as_slice(), "utf-16le"),
        (b"\xff\xfeA\0".as_slice(), "utf-8"),
        (b"\xfe\xff\0A".as_slice(), "utf-16le"),
    ] {
        let (_dir, left, right, output) = fixture(b"a\n", data);
        fs::write(&output, "keep").unwrap();
        let result = execute_encoded(&left, &right, &output, &["--force", "--er", encoding]);
        assert_eq!(result.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&result.stderr).contains("right file.csv"));
        assert_eq!(fs::read_to_string(&output).unwrap(), "keep");
        fs::remove_file(&output).unwrap();
        let result = execute_encoded(&left, &right, &output, &["--er", encoding]);
        assert_eq!(result.status.code(), Some(2));
        assert!(!output.exists());
    }
    let (_dir, left, right, output) = fixture(b"a\n", b"b\n");
    let result = execute_encoded(&left, &right, &output, &["--el", "auto"]);
    assert_eq!(result.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&result.stderr).contains("--list-encodings"));
    assert!(!output.exists());
}

#[test]
fn encoding_listing_needs_no_paths_and_help_shows_aliases() {
    let result = cli(&[std::ffi::OsStr::new("--list-encodings")]);
    assert_eq!(result.status.code(), Some(0));
    let list = String::from_utf8(result.stdout).unwrap();
    for (name, aliases) in celldiff::encoding::ENCODINGS {
        assert!(list.contains(name));
        for alias in *aliases {
            assert!(list.contains(alias));
        }
    }
    let help = cli(&[std::ffi::OsStr::new("--help")]);
    let help = String::from_utf8(help.stdout).unwrap();
    for name in [
        "--encoding-left",
        "--encoding-right",
        "--el",
        "--er",
        "--list-encodings",
    ] {
        assert!(help.contains(name), "{name}: {help}");
    }
}
