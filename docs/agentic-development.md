# Agentic Development Guide

## Purpose

This guide defines how agents should safely evolve Beeno while preserving CLI stability, safety guarantees, and publishable docs quality.

## Core Rules

1. Preserve execution safety
- Keep permission checks enforced before running generated code.
- Never bypass policy checks for translated output.

2. Keep config compatibility
- `.beeno.toml` is canonical.
- Preserve precedence: CLI > env > local > home > defaults.
- When adding config keys, provide defaults and tests.

3. Add tests with every behavior change
- New command/flag: add parser + behavior tests.
- New provider: add response-shape and error-path tests.
- New daemon/self-heal behavior: add integration tests.

4. Keep docs shippable
- Update README for user-facing command changes.
- Update `docs/architecture.md` for module/flow changes.
- Add feature docs for new workflows (`dev`, self-heal, providers).

## Change Checklist

- API/CLI changed?
  - Update help text and README command list.
- Config changed?
  - Update `.beeno.toml` template and config tests.
- Runtime behavior changed?
  - Add/adjust unit + integration coverage.
- Safety changed?
  - Revalidate protected files, path boundaries, and permissions.

## Preferred Delivery Pattern

1. Add a minimal safe implementation.
2. Add tests for core and failure paths.
3. Add docs and examples.
4. Only then expand capability.
