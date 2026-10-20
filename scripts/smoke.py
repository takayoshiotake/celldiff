#!/usr/bin/env python3
"""Run the packaged binary using only OS/Python test tooling."""
import argparse
import json
from pathlib import Path
import subprocess
import tempfile
import zipfile
import xml.etree.ElementTree as ET

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--binary", type=Path, required=True)
args = parser.parse_args()
binary = args.binary.resolve(strict=True)
with tempfile.TemporaryDirectory() as folder:
    output = Path(folder) / "sample.xlsx"
    command = [str(binary), "examples/left.csv", "examples/right.csv", "-o", str(output)]
    result = subprocess.run(command, text=True, capture_output=True)
    assert result.returncode == 1, result.stderr
    assert "changed_cells=7 " in result.stdout, result.stdout
    with zipfile.ZipFile(output) as archive:
        assert archive.testzip() is None
        sheet = archive.read("xl/worksheets/sheet1.xml").decode()
        assert "FFB91C1C" in sheet and "FF1D4ED8" in sheet
        ns = {"s": "http://schemas.openxmlformats.org/spreadsheetml/2006/main"}
        grid = ET.fromstring(sheet)
        assert grid.find("s:dimension", ns).get("ref") == "A1:C5"
        def cell_text(address):
            cell = grid.find(f".//s:c[@r='{address}']", ns)
            return "".join(node.text or "" for node in cell.findall(".//s:t", ns))
        assert cell_text("A1") == "id"
        assert cell_text("A2") == "001"
        assert cell_text("B2") == "東京大阪本社"
        assert cell_text("A5") == "004"
    again = subprocess.run(command, text=True, capture_output=True)
    assert again.returncode == 2
    identical = subprocess.run([str(binary), "examples/left.csv", "examples/left.csv", "-o",
        str(Path(folder) / "equal.xlsx")], text=True, capture_output=True)
    assert identical.returncode == 0
    left = Path("examples/left.csv").resolve()
    right = Path("examples/right.csv").resolve()
    existing = set(Path(folder).iterdir())
    for options, expected in [
        (["--text"], "B2:\n"),
        (["--text", "--oneline"], 'B2: "東京本社" -> "大阪本社"'),
        (["--text", "--oneline", "--address", "rc"], 'row=2 column=2: "東京本社" -> "大阪本社"'),
    ]:
        text = subprocess.run([str(binary), str(left), str(right), *options],
            cwd=folder, encoding="utf-8", capture_output=True)
        assert text.returncode == 1, text.stderr
        assert expected in text.stdout, text.stdout
        assert "Changed cells: 7 " in text.stdout, text.stdout
        assert set(Path(folder).iterdir()) == existing
    text_equal = subprocess.run([str(binary), str(left), str(left), "--text"],
        cwd=folder, encoding="utf-8", capture_output=True)
    assert text_equal.returncode == 0 and text_equal.stdout == "No differences.\n"
    quote_left = Path(folder) / "quote-left.csv"
    quote_right = Path(folder) / "quote-right.csv"
    quote_left.write_bytes(b'1,\n')
    quote_right.write_bytes(b'"1",""\n')
    default_quotes = subprocess.run([str(binary), str(quote_left), str(quote_right), "--text"],
        cwd=folder, encoding="utf-8", capture_output=True)
    assert default_quotes.returncode == 0 and default_quotes.stdout == "No differences.\n"
    quoted = subprocess.run([str(binary), str(quote_left), str(quote_right), "--compare-quotes", "--text", "--oneline"],
        cwd=folder, encoding="utf-8", capture_output=True)
    assert quoted.returncode == 1, quoted.stderr
    assert 'A1: "1" [unquoted] -> "1" [quoted]' in quoted.stdout
    assert 'B1: "" [unquoted] -> "" [quoted]' in quoted.stdout
    assert "Changed cells: 2 (modified=2, added=0, removed=0)" in quoted.stdout
    quote_output = Path(folder) / "quotes.xlsx"
    quoted_xlsx = subprocess.run([str(binary), str(quote_left), str(quote_right), "--compare-quotes", "-o", str(quote_output)],
        encoding="utf-8", capture_output=True)
    assert quoted_xlsx.returncode == 1, quoted_xlsx.stderr
    with zipfile.ZipFile(quote_output) as archive:
        quote_grid = ET.fromstring(archive.read("xl/worksheets/sheet1.xml"))
        quote_cell = quote_grid.find(".//s:c[@r='A1']", ns)
        assert "".join(n.text or "" for n in quote_cell.findall(".//s:t", ns)) == '"1"'
    target = Path("dist/sample.xlsx")
    target.parent.mkdir(exist_ok=True)
    target.write_bytes(output.read_bytes())
    print(json.dumps({"binary": str(binary), "sample_xlsx_bytes": output.stat().st_size,
                      "stdout": result.stdout.strip(), "smoke": "passed"}))
