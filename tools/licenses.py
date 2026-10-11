#!/usr/bin/env python3
"""List the licenses of every external crate in Cargo.lock (via cargo metadata)
and fail if any crate has no license, or only a copyleft license with no
permissive option. Run from the repository root: python3 tools/licenses.py"""
import collections
import json
import subprocess
import sys

PERMISSIVE = ("MIT", "Apache-2.0", "BSD", "ISC", "Zlib", "Unlicense", "CC0",
              "Unicode-3.0", "0BSD", "BSL-1.0")


def main() -> int:
    raw = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--locked"],
        check=True, capture_output=True, text=True).stdout
    meta = json.loads(raw)
    count = collections.Counter()
    bad = []
    for p in meta["packages"]:
        if p["source"] is None:
            continue  # workspace member
        lic = p.get("license") or "NONE"
        count[lic] += 1
        if not any(k in lic for k in PERMISSIVE):
            bad.append((p["name"], p["version"], lic))
    for lic, n in count.most_common():
        print(f"{n:4} {lic}")
    if bad:
        print("NEEDS REVIEW:", *bad, sep="\n  ")
        return 1
    print(f"ok: {sum(count.values())} external crates, all with a permissive option")
    return 0


if __name__ == "__main__":
    sys.exit(main())
