# oca workspace commands.
#
# On Windows without `make` on PATH, MinGW-w64 distributions (e.g. WinLibs) typically bundle
# it as `mingw32-make` instead — run `mingw32-make <target>` in that case. Every target below
# is a thin wrapper around a single cargo/rustup command, so running that command directly
# works too if you'd rather skip make entirely.

# ---------------------------------------------------------------------------
# Platform-specific library paths
#
# Set FFMPEG_DIR and (where needed) PKG_CONFIG_PATH in your environment, or
# let the platform block below provide sensible defaults based on common
# install locations.  Nothing here is user-specific — adjust your shell's
# environment instead of editing this file.
# ---------------------------------------------------------------------------

_OS := $(shell uname -s 2>/dev/null || echo Windows)

ifeq ($(_OS),Darwin)
  # macOS — Homebrew (Apple Silicon: /opt/homebrew, Intel: /usr/local)
  _BREW_PREFIX := $(shell brew --prefix 2>/dev/null || echo /opt/homebrew)
  FFMPEG_DIR    ?= $(_BREW_PREFIX)/opt/ffmpeg
  export FFMPEG_DIR
  export PATH := $(FFMPEG_DIR)/lib:$(_BREW_PREFIX)/bin:$(PATH)
  ifneq ($(wildcard $(CURDIR)/vendor/python-runtime/bin/python3.10),)
    PYO3_PYTHON ?= $(CURDIR)/vendor/python-runtime/bin/python3.10
    export PYO3_PYTHON
  endif
else ifeq ($(_OS),Linux)
  # Linux — pkg-config handles discovery; FFMPEG_DIR falls back to /usr.
  FFMPEG_DIR ?= $(shell pkg-config --variable=prefix libavformat 2>/dev/null || echo /usr)
  export FFMPEG_DIR
  export PATH := $(FFMPEG_DIR)/bin:$(PATH)
  ifneq ($(wildcard $(CURDIR)/vendor/python-runtime/bin/python3.10),)
    PYO3_PYTHON ?= $(CURDIR)/vendor/python-runtime/bin/python3.10
    export PYO3_PYTHON
  endif
else
  # Windows — avbridge automatically discovers a shared FFmpeg development installation when
  # its bin/ directory is on PATH. FFMPEG_DIR remains an optional explicit override.
  ifneq ($(wildcard $(CURDIR)/vendor/python-runtime/python.exe),)
    PYO3_PYTHON ?= $(CURDIR)/vendor/python-runtime/python.exe
    export PYO3_PYTHON
  endif
endif

# Cache dependency compilation across clean builds when sccache is installed (cargo install sccache).
# Measured on core's tests with an emptied target dir: 3m24s with a cold cache, 2m05s with a warm one.
# Workspace crates built incrementally are skipped by sccache, so edit-compile loops are unaffected.
ifneq ($(shell sccache --version 2>/dev/null),)
  export RUSTC_WRAPPER ?= sccache
endif

.PHONY: coverage coverage-web build release test test-core test-bush test-xtask test-e2e test-e2e-headless bench bench-bush fmt fmt-check lint lint-c fmt-c graph docs check clean desktop-install lint-desktop lint-desktop-fix desktop-run desktop-dev desktop-build test-desktop

# Debug build of the whole workspace (core, avbridge, ytbridge, desktop, xtask).
build:
	cargo build --workspace

# Optimized build. Binary ends up at target/release/oca-desktop(.exe) -- but see
# `desktop-build` for the one that actually embeds the built frontend (never a bare
# `cargo build --release`, see apps/desktop/CLAUDE.md).
release:
	cargo build --workspace --release

# Run every crate's test suite (core's ffprobe/loudness parsers, xtask's codegraph scanner).
test:
	cargo test --workspace

test-core:
	cargo test -p core

test-xtask:
	cargo test -p xtask

# Tauri shell (apps/desktop): React/TypeScript webview over the same avcore engine, migrating
# the egui screens one by one -- see spec/architecture/tauri-migration.md. Node/npm required.
desktop-install:
	cd apps/desktop && npm install

# One-shot launcher for manual validation: installs npm dependencies on first use, then starts
# the app (Vite + Tauri, hot reload). On Windows it also points bindgen at libclang (core's
# whisper-rs needs it; override with LIBCLANG_PATH) and puts FFmpeg's DLLs on PATH when FFMPEG_DIR
# is set. It goes through the Tauri CLI because a bare `cargo build -p desktop` embeds the
# dev-server URL instead of the built frontend. It runs the optimized (non-LTO) profile from
# FAST_TEST_ENV: a debug build aborts at startup on Windows (the debug CRT heap validator trips in
# whisper.cpp's static initialization, see spec/architecture/tauri-migration.md), and a full LTO
# release build takes minutes per change.
ifeq ($(OS),Windows_NT)
  LIBCLANG_PATH ?= C:/Program Files/LLVM/bin
  export LIBCLANG_PATH
  ifneq ($(FFMPEG_DIR),)
    export PATH := $(FFMPEG_DIR)/bin;$(PATH)
  endif
endif

desktop-run:
	@test -d apps/desktop/node_modules || (cd apps/desktop && npm install)
	cd apps/desktop && $(FAST_TEST_ENV) CARGO_TARGET_DIR="$(CURDIR)/target/fast" npm run tauri dev -- --release

# Runs Vite's dev server and the Tauri app with hot reload.
desktop-dev:
	cd apps/desktop && npm run tauri dev

# Release bundle (installer under target/release/bundle/).
desktop-build:
	cd apps/desktop && npm run tauri build

# Test builds of the Rust shell run optimized (its debug test binary aborts at startup on Windows
# with an MSVC CRT assertion) but without `[profile.release]`'s whole-program LTO and single codegen
# unit, which made every rebuild take minutes: the same optimized build, 16 codegen units, no LTO,
# incremental, in its own target dir (`target/fast`, so it never invalidates real release builds).
# Measured: touching one test file and re-running `cargo test -p desktop --no-run` takes ~9 s here
# instead of ~6.5 min.
FAST_TEST_ENV = CARGO_PROFILE_RELEASE_LTO=false CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 	CARGO_PROFILE_RELEASE_OPT_LEVEL=1 CARGO_PROFILE_RELEASE_PANIC=unwind 	CARGO_PROFILE_RELEASE_INCREMENTAL=true CARGO_TARGET_DIR=target/fast

# ESLint and Prettier over the React frontend; `lint-desktop-fix` applies the safe fixes.
lint-desktop:
	cd apps/desktop && npm run lint && npm run format:check

lint-desktop-fix:
	cd apps/desktop && npm run lint:fix && npm run format

# Frontend typecheck + unit tests, then the Rust shell's own tests.
test-desktop:
	cd apps/desktop && npm run typecheck && npm test
	$(FAST_TEST_ENV) cargo test -p desktop --release

# End-to-end tests (e2e/, pytest + selenium) driving the real built oca-desktop.exe through
# tauri-driver's WebDriver bridge. Needs a build first via `make desktop-build` (this target
# doesn't build one for you) -- never a bare `cargo build -p desktop`, which embeds the Vite dev
# URL instead of the real frontend (see e2e/conftest.py's header) -- plus `tauri-driver`
# installed once (`cargo install tauri-driver`) and Python deps installed once:
# `python -m pip install -r e2e/requirements.txt`.
test-e2e:
	cd e2e && python -m pytest

# Headless end-to-end tests against the real app through its control channel (no WebDriver); see
# e2e_headless/README.md. Needs the app built through the Tauri CLI and `cargo build -p oca-mcp`.
test-e2e-headless:
	cd e2e_headless && python -m pytest

# Runs the criterion benchmarks (parsing ffprobe/loudnorm output, project (de)serialization,
# timeline duration math) and writes an HTML report to target/criterion/report/index.html.
# Compare successive runs to catch performance regressions as the engine grows.
bench:
	cargo bench -p core

# Runs every Bush benchmark: scheduling/cache primitives plus the FFmpeg-backed engine workloads.
# Set BUSH_BENCH_MEDIA_DIR to a directory holding h264_1080p.mp4 / hevc_2160p.mp4 to include the
# large-media and multicam cases (see spec/matrix/bush.md).
bench-bush:
	cargo bench -p bush

# Bush test suite (needs FFMPEG_DIR). Add --features cpal for the device tests.
test-bush:
	cargo test -p bush

# Line coverage per crate (needs cargo-llvm-cov). Excluded by file, see
# spec/architecture/test-coverage-plan.md: the Tauri entry point, the audio device and its wrapper,
# the GPU renderer, the real network/self-update/model adapters (services.rs, update_install.rs, onnx_models.rs, whisper_model.rs),
# and the test files and test harness themselves. `desktop` runs in release because its debug test binary
# hits an MSVC CRT assertion at startup on Windows.
COVERAGE_IGNORE = (main\.rs|device_sink\.rs|gpu_[a-z_]+\.(rs|wgsl)|_test\.rs|[/\\]tests[/\\]|[/\\]testing\.rs|[/\\]services\.rs|bush_adapter[/\\]audio\.rs|update_install\.rs|onnx_models\.rs|whisper_model\.rs)
coverage:
	cargo llvm-cov -p avbridge --ignore-filename-regex '$(COVERAGE_IGNORE)'
	cargo llvm-cov -p bush --ignore-filename-regex '$(COVERAGE_IGNORE)'
	cargo llvm-cov -p core --ignore-run-fail --ignore-filename-regex '$(COVERAGE_IGNORE)'
	$(FAST_TEST_ENV) cargo llvm-cov -p desktop --release --ignore-filename-regex '$(COVERAGE_IGNORE)'

# Frontend line coverage (vitest + v8).
coverage-web:
	cd apps/desktop && npm run coverage

fmt: fmt-c
	cargo fmt --all

# Format the C bridge (`.clang-format`); the pre-commit hook does the same for staged files.
fmt-c:
	clang-format -i crates/avbridge/csrc/*.c crates/avbridge/csrc/*.h

# Check formatting without rewriting files — use in CI.
fmt-check:
	cargo fmt --all -- --check
	clang-format --dry-run -Werror crates/avbridge/csrc/*.c crates/avbridge/csrc/*.h

lint: lint-c
	cargo clippy --workspace --all-targets

# Static analysis of the C bridge (`.clang-tidy`); needs FFMPEG_DIR for the headers.
lint-c:
	for f in crates/avbridge/csrc/*.c; do clang-tidy $$f -- -I"$$FFMPEG_DIR/include" -Icrates/avbridge/csrc || exit 1; done

# Regenerate docs/CODE_GRAPH.md (module dependency graph + public API index). Run after
# adding/removing modules or public items.
graph:
	cargo run -p xtask -- graph

# Build and open the full rustdoc API reference for the workspace.
docs:
	cargo doc --workspace --no-deps --open

# Fast compile-only check (no codegen) — quicker feedback loop while editing.
check:
	cargo check --workspace --all-targets

clean:
	cargo clean
