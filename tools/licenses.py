#!/usr/bin/env python3
"""List the licenses of every external crate in Cargo.lock (via cargo metadata)
and fail if any crate has no license, or only a copyleft license with no
permissive option. Run from the repository root: python3 tools/licenses.py

With --notices PATH, also write the license and notice files of every
external crate into PATH, for shipping next to a binary (scripts/build.sh)."""
import argparse
import collections
import json
import pathlib
import subprocess
import sys

PERMISSIVE = ("MIT", "Apache-2.0", "BSD", "ISC", "Zlib", "Unlicense", "CC0",
              "Unicode-3.0", "0BSD", "BSL-1.0")


MIT_TEXT = """Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
"""


def standard_texts(p: dict) -> str:
    """The standard texts of the licenses a crate names but ships no file for."""
    lic = p.get("license") or ""
    out = []
    if "MIT" in lic:
        authors = ", ".join(p.get("authors") or []) or f"the {p['name']} authors"
        out.append(f"\n--- MIT License (standard text) ---\nCopyright (c) {authors}\n\n{MIT_TEXT}")
    if "Apache-2.0" in lic:
        apache = pathlib.Path(__file__).resolve().parent.parent / "LICENSE"
        out.append("\n--- Apache License 2.0 (standard text) ---\n" + apache.read_text(encoding="utf-8"))
    if not out:
        out.append("(no license file in the crate package; see its license field)\n")
    return "".join(out)


def write_notices(meta: dict, out: str) -> None:
    """Concatenate each external crate's LICENSE*/COPYING*/NOTICE* files."""
    parts = ["Licenses of the third-party crates built into this program.\n"]
    for p in sorted(meta["packages"], key=lambda p: (p["name"], p["version"])):
        if p["source"] is None:
            continue
        root = pathlib.Path(p["manifest_path"]).parent
        files = sorted(f for f in root.iterdir() if f.is_file() and
                       f.name.upper().startswith(("LICENSE", "LICENCE", "COPYING", "NOTICE")))
        parts.append(f"\n{'=' * 72}\n{p['name']} {p['version']}: {p.get('license') or 'NONE'}\n")
        if not files:
            parts.append(standard_texts(p))
        for f in files:
            parts.append(f"\n--- {f.name} ---\n")
            parts.append(f.read_text(encoding="utf-8", errors="replace"))
    pathlib.Path(out).write_text("".join(parts), encoding="utf-8")


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--notices", metavar="PATH", help="also write all crate license texts to PATH")
    args = ap.parse_args()
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
    if args.notices:
        write_notices(meta, args.notices)
        print(f"wrote {args.notices}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
