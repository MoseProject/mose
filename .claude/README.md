# Claude Code Configuration for Rust + Tauri Video Editor

This directory is a context and workflow package for building a professional desktop video editor.

## Install

Copy `.claude/` to the repository root.

## Recommended workflow

1. Ask Claude to inspect `CLAUDE.md`.
2. For a feature, first invoke the relevant domain and UX guidance.
3. Design before implementation.
4. Implement using project patterns.
5. Run Rust quality gates.
6. Capture screenshots and perform visual QA.
7. Profile performance for high-frequency interactions.

## Important

This package is intentionally opinionated. It treats the timeline as a high-performance interactive canvas rather than a generic list of UI widgets.
