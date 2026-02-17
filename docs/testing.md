# Testing Strategy

## Goals

Beeno should be safe to extend by humans and agents without regressions in execution, translation, server mode, or config behavior.

## Test Layers

1. Unit tests
- Config defaults and merge precedence.
- Provider endpoint selection and parsing helpers.
- CLI command parsing (`repl`, `run`, `dev`, `init-config`).
- Engine classifier, policy, and tagged NL transforms.

2. Integration tests
- `run` flow with tagged NL blocks and permissions.
- `dev` startup behavior from scaffold and from file input.
- Provider contracts (mocked HTTP responses for Ollama/OpenAI-compatible providers).
- Self-heal dry-run/apply once implemented.

3. E2E smoke tests
- Fixture projects that intentionally fail (`compile`, `runtime`, `missing module`).
- Assert output envelopes, artifacts, and retry behavior.

## Current Gaps

- No full integration/e2e suite committed yet.
- `cargo test` in this environment may fail due crates.io network restrictions.

## Recommended Commands

- `cargo fmt --all -- --check`
- `cargo test`
- `cargo clippy --workspace --all-targets -- -D warnings` (add in CI)

## Fixture Layout (planned)

- `tests/fixtures/broken_ts_compile`
- `tests/fixtures/runtime_reference_error`
- `tests/fixtures/missing_import`
