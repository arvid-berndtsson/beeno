# beeno

`beeno` is a Rust CLI wrapper that uses the `deno` binary as execution backend and adds
LLM-assisted pseudocode support for `repl`, `eval`, and `run`.

## Commands

- `beeno init-config [--force]`
- `beeno repl [--provider <id>] [--model <name>] [--policy <path>] [--json]`
- `beeno eval "<input>" [--json]`
- `beeno run <file> [--json]`

## Configuration

Beeno uses `.beeno.toml` in project root, with optional fallback to `~/.beeno.toml`.

Precedence order:

1. CLI flags
2. Environment variables
3. Local `.beeno.toml`
4. Home `~/.beeno.toml`
5. Built-in defaults

Provider support:

- `provider = "ollama"` for local models (`endpoint` default: `http://127.0.0.1:11434/api/generate`)
- `provider = "chatgpt"` for OpenAI Chat Completions API
- `provider = "openrouter"` for OpenRouter Chat Completions API
- `provider = "openai_compat"` for custom OpenAI-compatible URLs
- `provider = "http"` for legacy custom endpoint returning `{ "code": "..." }`
- `provider = "mock"` for local testing

Use `llm.endpoint` (or env var referenced by `llm.endpoint_env_var`) to override provider URL.

## Notes

- Native JS/TS is classified and executed without translation when possible.
- Pseudocode is translated through a provider adapter before AST policy checks.
- Tagged script blocks (`/*nl ... */`) are translated and inlined.
- REPL supports background server workflow:
  - `/serve-js <code>` / `/serve-nl <pseudocode>`
  - `/serve-hotfix-js <code>` / `/serve-hotfix-nl <pseudocode>`
  - `/serve-status`, `/serve-stop`, `/serve-port <port>`
  - prompts to open the hosted page in your default browser
