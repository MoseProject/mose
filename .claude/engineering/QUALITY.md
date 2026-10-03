# Quality Gates

Real commands (see repo-root `CLAUDE.md` for the authoritative source — this is a pointer, not a
fork, so it doesn't drift):

```bash
make build     # debug build, whole workspace
make run       # run the GUI app (debug)
make test      # run every crate's test suite
make test-core # cargo test -p core only
make test-e2e  # pytest + pywinauto against a built target/debug/ui.exe (Windows UI Automation)
make bench     # criterion benchmarks -> target/criterion/report/index.html
make fmt       # cargo fmt --all
make lint      # cargo clippy --workspace --all-targets
make check     # cargo check --workspace --all-targets (fast compile-only loop)
```

Single test: `cargo test -p core --test probe_test measure_loudness` or
`cargo test -p ui i18n::tests`.

Requires `FFMPEG_DIR` and (for `whisper-rs`) `LIBCLANG_PATH` set
per-shell — not persistent env vars on most dev machines. Do not assume a clean shell already has
these; export them in the same command chain as the `cargo`/`make` invocation.

## Test placement — real convention

`core` and `avbridge` have `[lib]` targets → integration tests in `crates/<crate>/tests/*.rs`.
`ui` is bin-only → unit tests live **beside** the module they test, in
`src/<module>/<module>_test.rs` (e.g. `crates/ui/src/app/app_test.rs`,
`crates/ui/src/screens/editor/timeline_panel/timeline_panel_test.rs`), included via
`#[cfg(test)] mod <module>_test;` at the bottom of the module file. Follow this placement for new
tests — don't create a top-level `tests/` directory under `ui`.

## Git hooks

`npm install` (one-time) sets up a husky pre-commit hook running `rustfmt`/`clang-format` on
staged `.rs`/`.c`/`.h` files via `lint-staged`. Skipping it just means no hook runs;
`make fmt`/`cargo fmt` still work standalone.

## Commit discipline (repo-wide convention, applies to UI work too)

Conventional Commits (`feat:`, `fix:`, `refactor:`, `test:`, `docs:`), split by
crate/layer in dependency order (`avbridge` → `core` → `ui` → `tests` → `docs`) even when
landing in the same session for one feature. A commit that only adds/changes tests for
already-committed code gets its own `test:`-prefixed commit.

## "Done" gate for a UI change specifically

Beyond the generic checklist in `CLAUDE.md`'s "Done criteria": if the change touches
`timeline_panel/`, explicitly state whether it adds per-frame per-clip cost, given the confirmed
no-viewport-culling gap (`engineering/TIMELINE_PERFORMANCE.md`) — "negligible, O(1) regardless of
clip count" or "adds O(clips), acceptable because <reason>" is an acceptable answer; silence on
the question is not.
