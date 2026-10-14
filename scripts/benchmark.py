#!/usr/bin/env python3
"""Reproducible Linux benchmark; requires /usr/bin/time and a release binary."""
import argparse
import csv
import json
import os
import stat
from pathlib import Path
import platform
import subprocess
import tempfile
import time

def bytes_in(paths, process_id):
    # Worksheet tempfiles may be unlinked while still open. Include /proc fds,
    # deduplicating by inode, so a directory listing does not undercount them.
    sizes = {}
    for path in paths:
        for file in path.rglob("*"):
            try:
                info = file.stat()
                if stat.S_ISREG(info.st_mode):
                    sizes[(info.st_dev, info.st_ino)] = info.st_size
            except FileNotFoundError:
                pass
    pending, visited = [process_id], set()
    while pending:
        pid = pending.pop()
        if pid in visited:
            continue
        visited.add(pid)
        try:
            children = Path(f"/proc/{pid}/task/{pid}/children").read_text()
            pending.extend(int(child) for child in children.split())
            for fd in Path(f"/proc/{pid}/fd").iterdir():
                try:
                    target = os.readlink(fd)
                    if not any(target.startswith(str(path) + "/") for path in paths):
                        continue
                    info = fd.stat()
                    if stat.S_ISREG(info.st_mode):
                        sizes[(info.st_dev, info.st_ino)] = info.st_size
                except (FileNotFoundError, PermissionError):
                    pass
        except (FileNotFoundError, PermissionError):
            pass
    return sum(sizes.values())

def measure(binary, rows, case):
    with tempfile.TemporaryDirectory(prefix="celldiff-bench-") as work:
        root = Path(work)
        tmp = root / "tmp"
        out = root / "output"
        tmp.mkdir()
        out.mkdir()
        old, new = root / "before.csv", root / "after.csv"
        expected = 0
        with old.open("w", newline="", encoding="utf-8") as a, new.open("w", newline="", encoding="utf-8") as b:
            wa, wb = csv.writer(a), csv.writer(b)
            for i in range(rows):
                value = "東京都" + ("a" * 5000 if case == "long" else "本社")
                changed = case == "dense" or (case in ("sparse", "long") and i % 100 == 0)
                wa.writerow([f"{i:08}", value, "=1+1", ""])
                wb.writerow([f"{i:08}", value.replace("東京都", "大阪府") if changed else value, "=1+1", ""])
                expected += int(changed)
        env = os.environ.copy()
        env["TMPDIR"] = str(tmp)
        stats = root / "time.txt"
        target = out / "report.xlsx"
        command = ["/usr/bin/time", "-f", "%e %M", "-o", str(stats), str(binary), str(old), str(new), "-o", str(target)]
        started = time.perf_counter()
        process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env)
        peak_disk = 0
        while process.poll() is None:
            peak_disk = max(peak_disk, bytes_in([tmp, out], process.pid))
            time.sleep(0.02)
        stdout, stderr = process.communicate()
        elapsed = time.perf_counter() - started
        peak_disk = max(peak_disk, bytes_in([tmp, out], process.pid))
        if process.returncode != int(expected > 0):
            raise RuntimeError(stderr.decode(errors="replace"))
        if f"changed_cells={expected} " not in stdout.decode():
            raise RuntimeError("Unexpected difference count")
        seconds, rss = stats.read_text().splitlines()[-1].split()
        return {
            "rows": rows, "case": case, "changed_cells": expected,
            "input_bytes": old.stat().st_size + new.stat().st_size,
            "elapsed_seconds": round(elapsed, 6), "time_elapsed_seconds": float(seconds),
            "peak_rss_kib": int(rss),
            "sampled_peak_temp_and_output_bytes": peak_disk,
            "xlsx_bytes": target.stat().st_size,
        }

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/release/celldiff"))
    parser.add_argument("--output", type=Path, default=Path("benchmark-results.json"))
    parser.add_argument("--rows", type=int, nargs="+", default=[10000, 100000])
    args = parser.parse_args()
    if any(n <= 0 for n in args.rows):
        parser.error("row counts must be positive")
    binary = args.binary.resolve()
    results = [measure(binary, rows, case) for rows in args.rows for case in ["equal", "sparse", "dense"]]
    results.append(measure(binary, min(max(args.rows), 10000), "long"))
    document = {"platform": platform.platform(),
                "rustc": subprocess.check_output(["rustc", "--version"], text=True).strip(),
                "commit": subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
                "binary_bytes": binary.stat().st_size,
                "temp_disk_sampling_seconds": 0.02, "results": results}
    args.output.write_text(json.dumps(document, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps(document, ensure_ascii=False, indent=2))

if __name__ == "__main__":
    main()
