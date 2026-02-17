# Architecture

## Workspace

- `crates/cli`: CLI, config loading, command orchestration
- `crates/core`: engine, repl, providers, server daemon management, and shared types

## Runtime Flow

1. Input enters CLI surface (`repl`, `eval`, or `run`).
2. Engine classifies code vs pseudocode.
3. Pseudocode gets translated by provider adapter.
4. Generated source is parsed by `deno_ast` and checked by policy.
5. Safe output executes via `deno run` with mapped permission flags.
6. Risky output requires explicit confirmation.
7. Blocked output is rejected with retry guidance.
