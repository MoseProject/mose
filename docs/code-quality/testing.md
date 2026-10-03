# Testing quality

> Load when: behavior changes, tests are written or reviewed, or acceptance evidence is assessed.
> Skip when: a documentation-only change has no executable contract.

## Test the contract

- Test observable behavior and invariants, not private implementation steps.
- Every bug fix needs a regression test that fails for the original defect when practical.
- Cover the happy path, meaningful boundaries, invalid input, cancellation, and partial failure.
- Assert exact state transitions and outputs. Avoid tests that only assert "no panic" or "is ok."
- Keep fixtures minimal but realistic enough to exercise the decoder, serializer, FFI, or UI boundary
  under test.

## Test level

- Use unit tests for pure logic and private helpers whose edge cases need fast feedback.
- Use `core` and `avbridge` integration tests for public contracts and real cross-module behavior.
- Use UI unit tests for `OcaApp` state transitions and command handling.
- Use end-to-end UI automation for accessibility names, focus, real navigation, and interactions that
  unit tests cannot prove.
- Use real FFmpeg/Bush pipeline tests where mocks would hide format, timing, ownership, or ABI
  failures. State clearly when the environment prevents that validation.

Follow the repository's test placement rules in `AGENTS.md`; do not create a new test layout for one
feature.

## Determinism and isolation

- Avoid wall-clock sleeps. Use bounded polling, injected clocks, explicit events, or timeouts with a
  diagnostic failure message.
- Do not depend on test order, shared mutable globals, network availability, or user-specific paths.
- Give each test unique temporary files and clean them through RAII.
- Correlate asynchronous events by stable identifiers and assert stale results are ignored.
- For concurrency, test completion, cancellation, capacity, and shutdown, not a lucky interleaving.

## Quality checks

- A test should fail for one understandable reason and name the behavior it proves.
- Table-driven cases should keep each case readable and identify which input failed.
- Property tests are appropriate for serialization round trips, timeline invariants, ranges, and
  arithmetic over broad input spaces.
- Snapshot or golden tests need a focused artifact and an intentional review process; do not use them
  to hide large unexplained output changes.
- Avoid excessive mocking. Mock external nondeterminism or expensive boundaries, not the domain logic
  being verified.

## Review questions

1. Does every acceptance criterion have executable evidence at the correct layer?
2. Would the test detect the likely regression, or only the current implementation shape?
3. Are error and cancellation paths covered?
4. Can it pass locally but fail nondeterministically in CI?
5. Is real-pipeline or end-to-end validation required before claiming the feature works?
