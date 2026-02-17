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

## Issue Drafts (for GitHub)

- [ ] `feat(self-heal): auto-trigger dry-run suggestions on run failure`
- [ ] `feat(artifacts): persist suggestion reports and diagnostics in .beeno/suggestions`
- [ ] `feat(safety): enforce protect.deny and patch budget limits before apply`
- [ ] `feat(apply): add apply-fixes retry loop with git safeguards`
- [ ] `test(integration): add self-heal dry-run/apply flow coverage with fixtures`
- [ ] `test(retention): add artifact pruning tests honoring artifacts.keep_last`
- [ ] `docs(self-heal): add operator guide and CLI examples`
