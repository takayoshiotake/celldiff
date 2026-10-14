"""Temporary bootstrap: retrieve Cargo-generated lock and rustfmt output through CI logs."""
import json
from pathlib import Path

paths = [Path("Cargo.lock"), *Path("src").rglob("*.rs"), *Path("tests").rglob("*.rs")]
print("CELLDIFF_CAPTURE=" + json.dumps({str(path): path.read_text() for path in paths}))
