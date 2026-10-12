#!/usr/bin/env bash
# Clean release build of the game from a fresh checkout.
#   scripts/build.sh            build, self-test, and collect files in dist/
# Needs: Rust (stable, https://rustup.rs) and a C compiler (SQLite is built
# from source). Works on Linux, macOS and Windows (Git Bash).
set -euo pipefail
cd "$(dirname "$0")/.."

command -v cargo >/dev/null || { echo "cargo not found: install Rust from https://rustup.rs" >&2; exit 1; }

cargo build --release --locked -p ui

exe=target/release/game
[ -f "$exe" ] || exe=target/release/game.exe
"$exe" --self-test

rm -rf dist
mkdir -p dist
cp "$exe" dist/
cp README.md LICENSE NOTICE THIRD_PARTY.md dist/
py=python3
command -v "$py" >/dev/null || py=python
"$py" tools/licenses.py --notices dist/THIRD_PARTY_LICENSES.txt
echo "built: dist/$(basename "$exe")"
