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
    command = [str(binary), "examples/before.csv", "examples/after.csv", "-o", str(output)]
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
    identical = subprocess.run([str(binary), "examples/before.csv", "examples/before.csv", "-o",
        str(Path(folder) / "equal.xlsx")], text=True, capture_output=True)
    assert identical.returncode == 0
    target = Path("dist/sample.xlsx")
    target.parent.mkdir(exist_ok=True)
    target.write_bytes(output.read_bytes())
    print(json.dumps({"binary": str(binary), "sample_xlsx_bytes": output.stat().st_size,
                      "stdout": result.stdout.strip(), "smoke": "passed"}))
