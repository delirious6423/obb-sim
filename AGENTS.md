# AGENTS.md

Shared rules for coding agents in this Rust project. The architecture rules, the crate map, and the working method are in `CLAUDE.md`. Read them before you change code. The architecture rules apply to every agent. Longer procedures are in `docs/rust-dev.md`.

## Project

- Purpose: closed-source, OOTP-style baseball management sim. Fictional players and teams only.
- Workspace: the members are listed in the root `Cargo.toml`. The binary `sim` is in `crates/cli`.
- Edition 2024, resolver 3. No `rust-version` and no `rust-toolchain.toml` are set. CI uses the stable toolchain on Linux, macOS, and Windows.
- Async runtime: none. Global allocator: none set. Parallel code uses `rayon`.
- Lints: the workspace forbids `unsafe_code`. `clippy.toml` bans the std `HashMap` and `HashSet`.

## Commands

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                          # the CI gate; it also runs doctests
cargo test --release -- --ignored calibration   # Phase 1 gate; slow; needs data/internal/params.json
cargo run --release -p cli -- bench 20          # speed baseline
```

Optional checks. CI does not run them:

```bash
cargo nextest run --workspace         # does not run doctests
cargo llvm-cov nextest --workspace    # coverage
cargo machete                         # unused dependencies: review before removing
cargo deny check                      # needs deny.toml; first run downloads the advisory database
```

- CI sets `RUSTFLAGS=-D warnings`.
- Each phase has its own gate command. See `PLAN.md`. The gates need `data/internal/params.json`.
- nextest skips ignored tests unless you pass `--run-ignored` with its value. Check `cargo nextest run --help`.
- Run `bench` from the repository root. Compare only runs that print the same parameter source.
- Use `cargo check -p <crate>` while editing. Run the broad checks before you finish.
- Do not use `--all-features` unless the features work together.

## Rules

- Do not commit or push unless the user asks or `PLAN.md` tells you to. `PLAN.md` asks for small commits and a push for each phase. Other sessions can edit the same tree. Stage files by name. Never use `git add -A` or `git add .`. Read `git diff --cached` before you commit. Run the first three commands in the Commands section first. Never commit `data/internal/`.
- Do not change user-level configuration (`~/.claude`, `~/.cargo`, shell profiles) or install global tools. If a tool is missing, tell the user.
- Read the docs of the resolved crate version before you use an unfamiliar API.
- Log every new dependency in `THIRD_PARTY.md`.
- Fix ownership and lifetime errors at the cause. Do not add `clone`, `Rc`/`Arc`, locks, or lifetime extensions without saying why. Do not add `unsafe`; the workspace forbids it.
- Test changed observable behavior, including error cases.
- Never change a gate threshold, golden value, or test to make it pass.
- Before you optimize, record a reproducible baseline with `cargo run --release -p cli -- bench 20`. For a rewrite, keep the old code or another oracle to compare outputs. Simulation results must stay identical for the same seed.
- Profiling is off by default. The workspace has no `hotpath` dependency yet. To profile, first add `hotpath` to `crates/cli` as a normal dependency with feature passthroughs. Then profile only with `--features hotpath`. Never add hotpath features to `default`.
- Do not add hotpath to `sim-core` or `season` until the user says so. They are pure. The hotpath feature starts a metrics server.
- Procedure: `docs/rust-dev.md`.
- Do not hide warnings or edit unrelated code to get a clean result.
- In reports, keep verified results, assumptions, and untested claims apart.
