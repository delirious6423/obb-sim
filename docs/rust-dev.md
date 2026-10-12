# Rust development procedures

This document gives the long procedures for this project. AGENTS.md gives the short rules and the Commands section. CLAUDE.md gives the architecture rules, the crate map, the working method, and the Claude-only tool notes. If a fact is not known, write "TODO: confirm in the project". Do not guess.

## 1. Required tools

- rustup
- Rust stable toolchain with the components rustfmt, clippy, rust-analyzer, rust-src, and llvm-tools
- Rust nightly toolchain
- cargo-nextest
- cargo-llvm-cov
- cargo-machete
- cargo-deny
- cargo-expand
- hotpath CLI
- fff-mcp
- rg
- rust-analyzer LSP plugin

Check each tool with `command -v <name>`. If a tool is missing, tell the user. Agents do not install or register tools.

## 2. Daily checks

Run the first three commands in the Commands section of AGENTS.md in order. Run the gate and the bench only when a step asks for them.

- Use `cargo check -p <crate>` while you edit.
- The project gate is `cargo test --workspace`, as in CI. CI also sets `RUSTFLAGS=-D warnings`. The Phase 1 gate is `cargo test --release -- --ignored calibration`. Each phase has its own gate. See `PLAN.md`. The gates need `data/internal/params.json`.
- nextest is optional. It skips ignored tests unless you pass `--run-ignored` with its value. Check `cargo nextest run --help`.
- nextest does not run doctests. The Commands section has the doctest command.
- `cargo deny check` needs a `deny.toml` file. The user creates it with `cargo deny init`. Do not write a dependency policy. The first run downloads the RustSec advisory database over the network.
- Review each cargo-machete result before you remove a dependency. Generated code can use a dependency that the tool cannot see.

## 3. Coverage

1. Measure the coverage of the workspace.

```bash
cargo llvm-cov nextest --workspace
```

Doctest coverage is not set up. It needs `cargo +nightly llvm-cov --doctests`, and the installed nightly has no llvm-tools component.

## 4. Search and code intelligence

Use the right tool for each search:

- rust-analyzer LSP plugin: symbols, types, definitions, and references. Use it before text search.
- fff MCP tools `find_files`, `grep`, and `multi_grep`: fuzzy file paths and repeated indexed searches. They index the directory where the Claude Code session starts.
- `rg`: exact strings.

If the LSP or fff tools are missing, tell the user. The user must start a new session, or close and open the Claude app again. Then check again. Start the session in the folder that holds `Cargo.toml`, so rust-analyzer can load the project. Do not register servers or install plugins.

## 5. Profile with hotpath

The hotpath profiler is off by default. The `hotpath` feature turns it on. Wall time, CPU time, allocation, and wait time are different measurements. Instrumentation adds overhead, so compare only runs made the same way. Do not invent numbers, and report only measured values.

Add hotpath to `crates/cli/Cargo.toml` as a normal dependency. The commands in this section fail until you do this. Merge the feature lines into the `[features]` table. Do not create a second table.

Project limits for this repository:

- `sim-core` and `season` are pure: no I/O, no clock, no global state. The plain `hotpath` feature starts a metrics server. So do not add hotpath to these crates until the user says so.
- Open decision: the user decides this. The hot loop is in `sim-core`. Until then, a profile shows only `cli` functions.
- Log hotpath in `THIRD_PARTY.md` when you add it.
- The simulation results must be identical with the feature on and off. The `bench` timings change on every run. Compare the `digest` values.
- Use `cargo run --release -p cli -- bench 20` as the baseline. Run it from the repository root. Compare only runs that print the same parameter source.

```toml
[dependencies]
hotpath = "0.28"

[features]
hotpath = ["hotpath/hotpath"]
hotpath-alloc = ["hotpath/hotpath-alloc"]
hotpath-mcp = ["hotpath/hotpath-mcp"]
```

- Never add a hotpath feature to `default`. Never edit the `default` list.
- With the features off, the macros have no effect. So hotpath is a normal dependency, not an optional one.
- The optional form, for a project that does not allow extra crates, needs `cfg_attr` on every use.

Use the `hotpath_init` skill for instrumentation. Do not run `hotpath init` or `hotpath init-ci` in a session, because both start agent sessions.

- Add `#[hotpath::main]` to `main` in `crates/cli/src/main.rs`. This project does not use Tokio.
- In the first pass, add `#[hotpath::measure]` to three to five functions in total. Do not use `measure_all`.

The plain `hotpath` feature also starts a metrics server on 127.0.0.1:6770. Disable it with the environment variable `HOTPATH_METRICS_SERVER_OFF`. The `hotpath console` command needs a CLI built with the `tui` feature. The installed CLI does not have that feature.

### Run the profile

1. Run the program with the `hotpath` feature.

```bash
cargo run --release -p cli --features hotpath -- bench 20
```

2. Read the report. The report appears when the program exits.
3. Record the command, input, build profile, features, toolchain, and hardware.

### Allocation run

Allocation is a separate second run. It conflicts with any `#[global_allocator]` in the crate.

1. Check the crate for `#[global_allocator]`. None existed when this file was written.
2. If the attribute exists, tell the user. Do not run step 3.
3. Run the program with both features.

```bash
cargo run --release -p cli --features hotpath,hotpath-alloc -- bench 20
```

### Live data for Claude

This section does not apply now, because this project has no long-running program yet. Live data needs the `hotpath-mcp` feature and an app that stays open. The MCP server listens on 127.0.0.1:6771. It does not work for a short program that exits. Set `HOTPATH_MCP_PORT` to change the port. If `HOTPATH_MCP_AUTH_TOKEN` is set, the client must send the token in an `Authorization` header.

1. Run the app with the `hotpath-mcp` feature. Keep the app open.

```bash
# TODO: confirm in the project. No long-running command exists yet.
```

2. Register the server only when the user asks.

```bash
claude mcp add --transport http hotpath http://127.0.0.1:6771/mcp
```

## 6. Build cache

kache is not set up. Do not install or configure it unless the user asks.

A bounded trial needs these items:

- A separate target directory.
- A preview with `kache init --check` before any change.
- No background service and no C or C++ shims.
- A record of hits, misses, and time for each build.

Expect few cache hits when incremental compilation is on. The kache README does not state how kache works with incremental compilation. TODO: confirm in the project.

## 7. Known gaps

- Doctest coverage is not set up. See section 3.
- The project has no `deny.toml` or `rust-toolchain.toml` file yet.
- The hotpath MCP link is not tested.
- Go-to-definition and hover are not checked on this project. Check them in the first session that opens it.
- The kache trial is not done. TODO: confirm in the project how to undo `kache init`.
- The workspace sets no `rust-version`. Edition 2024 needs Rust 1.85 or newer.
- CI does not run nextest, llvm-cov, machete, or deny.
- Hotpath in the pure crates `sim-core` and `season` needs a decision from the user. See section 5.
