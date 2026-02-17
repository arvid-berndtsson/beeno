# beeno

`beeno` is a Rust CLI wrapper that uses the `deno` binary as execution backend and adds
LLM-assisted pseudocode support for `repl`, `eval`, and `run`.

## Commands

- `beeno init-config [--force]`
- `beeno repl [--provider <id>] [--model <name>] [--policy <path>] [--json]`
- `beeno dev [--file <path>] [--port 8080] [--open]`
- `beeno eval "<input>" [--json]`
- `beeno run <file> [--json]`

## Install via curl

Use the installer script (downloads the right release archive, verifies checksum, and installs
to `~/.local/bin` by default):

```bash
curl -fsSL https://raw.githubusercontent.com/arvid/beeno/main/install.sh | sh
```

Install a specific release:

```bash
curl -fsSL https://raw.githubusercontent.com/arvid/beeno/main/install.sh | VERSION=v0.2.0 sh
```

Install to a different path:

```bash
curl -fsSL https://raw.githubusercontent.com/arvid/beeno/main/install.sh | INSTALL_DIR=/usr/local/bin sh
```

The release workflow publishes archives named `beeno-v{version}-{target}.{tar.gz|zip}` plus
`SHA256SUMS.txt`. Windows assets are published as `beeno-v{version}-x86_64-pc-windows-msvc.zip`.

### Checksum verification examples

macOS:

```bash
shasum -a 256 -c SHA256SUMS.txt
```

Linux:

```bash
sha256sum -c SHA256SUMS.txt
```

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

## Documentation

- `docs/architecture.md` - module/runtime overview
- `docs/dev-mode.md` - `beeno dev` behavior and commands
- `docs/testing.md` - testing strategy and coverage targets
- `docs/agentic-development.md` - contributor/agent workflow guardrails

## Notes

- Native JS/TS is classified and executed without translation when possible.
- Pseudocode is translated through a provider adapter before AST policy checks.
- Tagged script blocks (`/*nl ... */`) are translated and inlined.
- REPL supports background server workflow:
  - `/serve-js <code>` / `/serve-nl <pseudocode>`
  - `/serve-hotfix-js <code>` / `/serve-hotfix-nl <pseudocode>`
  - `/serve-status`, `/serve-stop`, `/serve-port <port>`
  - prompts to open the hosted page in your default browser
- `beeno dev` starts a dedicated long-running dev server shell with hotfix commands:
  - `/status`, `/open`, `/restart`, `/hotfix-js`, `/hotfix-nl`, `/stop`, `/start`, `/quit`

## Maintainer release notes

- Release automation lives in `.github/workflows/release.yml`
- crates.io publish auth uses trusted publishing (GitHub OIDC), not a long-lived token.
- Configure trusted publishers on crates.io for both `beeno_core` and `beeno` to allow
  this repository/workflow/environment (`release`) to publish.
- Versioning is tag-driven: on release events, CI syncs Cargo versions from the release tag
  (for example `v0.2.0` -> `0.2.0`) before publish/build.
- On `Release -> Publish release`, CI runs license checks, publishes `beeno_core` then `beeno`
  to crates.io, builds Tier-1 binaries, generates `SHA256SUMS.txt`, and uploads all artifacts.
- Optional dry-run validation: run `release` via `workflow_dispatch` with
  `publish_crates = false` and optionally set `release_version` to simulate tag-driven builds.
