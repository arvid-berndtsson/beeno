# Beeno TODO

## P0 - Self-Heal Runtime

- [ ] Implement auto self-heal trigger on `beeno run` failure.
  - Default behavior: dry-run suggestions only.
  - Opt-in behavior: apply fixes with `--apply-fixes`.
- [ ] Add retry loop for apply mode.
  - Use configured attempt budget (`self_heal.max_attempts`, capped at 3 in v1).
- [ ] Implement failure analysis pipeline.
  - Parse and classify TypeScript compile errors, runtime exceptions, module resolution failures, and permission failures.

## P0 - Artifact Pipeline

- [ ] Create artifact directory on run failure: `.beeno/suggestions/<timestamp>/`.
- [ ] Persist dry-run outputs:
  - `report.json`
  - `proposed.patch`
  - `diagnostics.log`
- [ ] Persist apply-mode outputs:
  - `applied.patch`
  - git safeguard metadata file
- [ ] Implement retention pruning based on config (`artifacts.keep_last`).

## P0 - Safety and Validation

- [ ] Enforce protected file denylist (`protect.deny`) before applying any fix.
- [ ] Enforce patch size limits:
  - `limits.max_files`
  - `limits.max_changed_lines`
- [ ] Enforce project-root boundary (reject out-of-root edits/path traversal).
- [ ] Add Git safeguard requirement for `--apply-fixes`.

## P1 - Integration and E2E Tests

- [ ] Add integration tests for self-heal dry-run flow.
  - Failed run produces artifacts, no source edits.
- [ ] Add integration tests for self-heal apply flow.
  - Allowed edits apply and rerun path is exercised.
- [ ] Add integration test for protected-file rejection.
- [ ] Add integration test for retention pruning behavior.
- [ ] Add e2e fixture projects for common failure classes:
  - broken TS compile
  - runtime reference error
  - missing import/module

## P1 - CLI/UX Output

- [ ] Add clear dry-run summary output in terminal.
  - Root cause, proposed files, next command hint.
- [ ] Include healing artifact paths in JSON output (`report.json`, `proposed.patch`, `diagnostics.log`).
- [ ] Add explicit messaging for `--no-self-heal` behavior.

## P2 - Docs

- [ ] Document self-heal flow and safeguards in `docs/self-healing.md`.
- [ ] Add examples for:
  - default dry-run flow
  - apply flow with retries
  - disabling self-heal for a run

## P1 - Provider Reliability

- [ ] Add integration tests for provider implementations:
  - Ollama request/response handling
  - OpenAI-compatible (ChatGPT/OpenRouter/custom URL) parsing
  - Legacy HTTP provider parsing
- [ ] Add provider failure-mode tests:
  - auth errors
  - rate limits
  - malformed responses
  - timeout handling

## P1 - Provider Config Hardening

- [ ] Validate provider names early with clear CLI/config errors.
- [ ] Validate endpoint format and report actionable errors.
- [ ] Improve missing API key diagnostics per provider type.

## P1 - Secret Safety

- [ ] Ensure API keys/auth headers are never persisted in artifacts or logs.
- [ ] Add redaction for sensitive env-derived values in debug output.
- [ ] Add tests for redaction behavior.

## P1 - Self-Heal + Provider Integration

- [ ] Add tests that self-heal uses configured provider selection correctly.
- [ ] Add coverage for `.beeno.toml` + env precedence across provider switching.
- [ ] Verify Ollama/local defaults behave correctly in self-heal mode.

## P2 - Provider Docs and Ops

- [ ] Add provider-specific `.beeno.toml` examples for:
  - Ollama
  - ChatGPT
  - OpenRouter
  - generic OpenAI-compatible URL
- [ ] Add CI workflow to run fmt + tests in network-enabled environment.

## P1 - Release and Compatibility

- [ ] Pin and document minimum supported Deno version.
- [ ] Add compatibility tests across a small Deno version matrix.
- [ ] Document behavior differences across supported Deno versions (if any).

## P1 - Performance and Cost Controls

- [ ] Add provider-specific timeout/retry/backoff policies.
- [ ] Add optional caching/de-duplication for repeated translation prompts.
- [ ] Add tests for timeout/retry behavior and cache hits/misses.

## P1 - Additional Self-Heal Guardrails

- [ ] Add editable file-extension allowlist for auto-fix apply mode.
- [ ] Add max runtime budget per self-heal session.
- [ ] Add tests for allowlist and runtime-budget enforcement.

## P2 - Observability

- [ ] Add structured phase events (`classify`, `translate`, `run`, `heal-plan`, `heal-apply`).
- [ ] Add metrics counters for:
  - heal attempts
  - successful heals
  - blocked fixes
  - provider failures
- [ ] Expose observability output in JSON mode.

## P2 - Quality Gates

- [ ] Add `clippy` to CI.
- [ ] Add docs checks (README/config examples validity where possible).
- [ ] Add PR checklist for config/schema changes and migration notes.

## P1 - Security Hardening

- [ ] Add prompt-injection defenses for self-heal and translation prompts.
- [ ] Harden path normalization and symlink handling to prevent escape from project root.
- [ ] Add tests for path traversal and symlink escape scenarios.

## P1 - Data Governance

- [ ] Add configurable artifact redaction policy.
- [ ] Add option to disable artifact persistence for sensitive repositories.
- [ ] Add tests ensuring sensitive data is not persisted when redaction/disable flags are enabled.

## P1 - UX Resilience

- [ ] Define and enforce non-interactive/CI-safe behavior.
- [ ] Standardize exit-code contract across run/heal outcomes.
- [ ] Add integration tests asserting exit codes for success, dry-run suggested fixes, blocked fixes, and hard failures.

## P2 - Migration and Versioning

- [ ] Add config schema version field to `.beeno.toml`.
- [ ] Add schema migration handling for future config changes.
- [ ] Document migration policy in docs.

## P2 - Packaging and Distribution

- [ ] Define installation/distribution path (Homebrew/tap or install script).
- [ ] Publish signed release artifacts with checksums.
- [ ] Add release checklist documentation.

## Issue Drafts (for GitHub)

- [ ] `feat(self-heal): auto-trigger dry-run suggestions on run failure`
- [ ] `feat(artifacts): persist suggestion reports and diagnostics in .beeno/suggestions`
- [ ] `feat(safety): enforce protect.deny and patch budget limits before apply`
- [ ] `feat(apply): add apply-fixes retry loop with git safeguards`
- [ ] `test(integration): add self-heal dry-run/apply flow coverage with fixtures`
- [ ] `test(retention): add artifact pruning tests honoring artifacts.keep_last`
- [ ] `docs(self-heal): add operator guide and CLI examples`
