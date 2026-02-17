# beeno

`beeno` is a Rust CLI wrapper that uses the `deno` binary as execution backend and adds
LLM-assisted pseudocode support for `repl`, `eval`, and `run`.

## Commands

- `beeno repl [--provider <id>] [--model <name>] [--policy <path>] [--json]`
- `beeno eval "<input>" [--json]`
- `beeno run <file> [--json]`

## Notes

- Native JS/TS is classified and executed without translation when possible.
- Pseudocode is translated through a provider adapter before AST policy checks.
- Tagged script blocks (`/*nl ... */`) are translated and inlined.
