import json
from pathlib import Path
print("CELLDIFF_LAYOUT_CAPTURE=" + json.dumps({str(p): p.read_text() for p in [*Path("src").rglob("*.rs"), *Path("tests").rglob("*.rs")]}))
