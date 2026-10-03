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

.PHONY: build release test test-core test-bush test-xtask test-e2e bench bench-bush fmt fmt-check lint graph docs check clean desktop-install desktop-dev desktop-build test-desktop

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

# Runs Vite's dev server and the Tauri app with hot reload.
desktop-dev:
	cd apps/desktop && npm run tauri dev

# Release bundle (installer under target/release/bundle/).
desktop-build:
	cd apps/desktop && npm run tauri build

# Frontend typecheck + unit tests, then the Rust shell's own tests.
test-desktop:
	cd apps/desktop && npm run typecheck && npm test
	cargo test -p desktop

# End-to-end tests (e2e/, pytest + selenium) driving the real built oca-desktop.exe through
# tauri-driver's WebDriver bridge. Needs a build first via `make desktop-build` (this target
# doesn't build one for you) -- never a bare `cargo build -p desktop`, which embeds the Vite dev
# URL instead of the real frontend (see e2e/conftest.py's header) -- plus `tauri-driver`
# installed once (`cargo install tauri-driver`) and Python deps installed once:
# `python -m pip install -r e2e/requirements.txt`.
test-e2e:
	cd e2e && python -m pytest

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

fmt:
	cargo fmt --all

# Check formatting without rewriting files — use in CI.
fmt-check:
	cargo fmt --all -- --check

lint:
	cargo clippy --workspace --all-targets

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
