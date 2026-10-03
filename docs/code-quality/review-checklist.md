# Code review checklist

> Load when: reviewing a patch, pull request, or completed task.
> Then load only the topical guides selected by `README.md`.

## Review order

1. Read the request, acceptance criteria, and relevant spec before the diff.
2. Determine affected crates, trust boundaries, hot paths, and public contracts.
3. Inspect the complete diff and enough surrounding code to validate each changed assumption.
4. Run or inspect the narrowest meaningful validation, then expand for shared or cross-crate changes.
5. Report only actionable findings introduced or exposed by the change.

## Required gates

- **Behavior:** implementation matches the requested semantics, including failure and cancellation.
- **Architecture:** dependency direction remains `avbridge -> core -> ui`; mutations use owning model
  APIs; no unrelated refactor is mixed into the task.
- **Design:** relevant KISS, DRY, YAGNI, SOLID, cohesion, and coupling checks from
  `design-principles.md` pass without demanding speculative abstractions.
- **Rust/C/FFI:** ownership, errors, unsafe contracts, native resources, and integer/buffer boundaries
  are correct.
- **Performance:** changed hot paths have bounded work and resources; optimization claims have
  evidence.
- **Security:** every changed trust boundary validates data and preserves authenticity, path, secret,
  and command-safety guarantees.
- **Tests:** acceptance criteria and regression risks have deterministic evidence at the right layer.
- **Documentation:** public behavior, important invariants, and spec/matrix status are current.
- **Validation:** formatting, compilation, lint, tests, and real-pipeline checks are reported honestly.

## Severity

- **P0:** immediate data loss, arbitrary code execution, secret exposure, or release-blocking failure.
- **P1:** incorrect common-path behavior, memory unsafety, deadlock, unbounded resource growth, broken
  compatibility, or a security boundary bypass.
- **P2:** real edge-case defect, material maintainability regression, missing important test, or
  measurable performance regression with limited impact.
- **P3:** low-impact improvement with a concrete future failure mode. Pure preference is not a finding.

Approval is blocked by any open P0 or P1 finding. A P2 blocks when it violates stated acceptance
criteria or the definition of done. P3 findings should be sparse and actionable.

## Finding format

Each finding must state:

1. a short imperative title with severity;
2. the smallest relevant file and line range;
3. the concrete input or execution path that triggers the issue;
4. the observable impact and violated contract;
5. a correction direction, without rewriting unrelated code.

Do not report pre-existing issues unless the patch makes them reachable, worse, or necessary to fix
for the requested behavior. If there are no actionable findings, say so and list any validation that
could not run.

## Completion evidence

Record commands actually executed, their outcomes, and environmental gaps. "Compiled" is not
equivalent to "verified." For UI/media work, distinguish unit validation from real Bush/FFmpeg
or end-to-end validation. For performance, include baseline and after measurements or explicitly say
that no performance claim was made.
