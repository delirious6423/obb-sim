# tools

`build_params.py` downloads Retrosheet 2015-2019 event files into memory (nothing is written to disk
except the result), aggregates league-wide rates and talent spread, and writes
`data/internal/params.json` (git-ignored). No player id, name or stat line is stored.

    python3 -I -B tools/build_params.py

Standard library only. Data: see THIRD_PARTY.md for the Retrosheet notice.
