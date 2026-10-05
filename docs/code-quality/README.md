# On-demand code quality references

These documents are a review library, not a single prompt. Load only the files whose trigger
matches the current task. Do not preload the whole directory.

## Routing

| Task signal | Load | Skip when |
|---|---|---|
| New behavior, refactor, architecture change, or code review | [design-principles.md](design-principles.md) | Documentation-only or mechanical formatting changes |
| Rust API, ownership, concurrency, error handling, or unsafe code | [rust.md](rust.md) | No Rust code is touched |
| Playback, preview, import, export, decoding, rendering, cache, allocation, I/O, or a reported slowdown | [performance.md](performance.md) | No runtime path or resource use changes |
| Tests are added, changed, reviewed, or used as acceptance evidence | [testing.md](testing.md) | No behavior or test changes |
| Untrusted files, paths, downloads, dependencies, FFI, secrets, or user-controlled data | [security.md](security.md) | The change has no trust boundary |
| Planning a refactor, or asking what is oversized, duplicated or misplaced | [architecture-review-2026-10.md](architecture-review-2026-10.md): findings, what changed, backlog | The task is a bug fix or a feature in an already well-sized module |
| A patch or pull request is being reviewed | [review-checklist.md](review-checklist.md), then only the relevant guides above | Implementation is still being designed |

When multiple rows match, load their union once. Start with the smallest set and open another
guide only when the diff or investigation reveals its trigger.

## Precedence

1. Correctness and explicit product behavior.
2. Safety, data integrity, and security.
3. Measured performance requirements.
4. Maintainability and clarity.
5. Local elegance.

Principles are decision tools, not scoring targets. Do not introduce an abstraction merely to
claim SOLID or DRY compliance. A small, explicit implementation is preferable until repeated
variation proves that an abstraction has a stable boundary.

## Evidence standard

A review finding must include a concrete location, the violated contract or risk, its observable
impact, and the smallest reasonable correction. Do not report speculative style preferences as
defects. Performance claims require a benchmark, profile, complexity argument, or a clearly
identified unbounded resource path.
