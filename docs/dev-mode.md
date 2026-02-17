# Dev Mode (`beeno dev`)

`beeno dev` is the long-running shell for background web-server workflows.

## Command

- `beeno dev [--file <path>] [--port 8080] [--open]`

## Behavior

- Starts a background Deno server process.
- If `--file` is provided, uses that file as source.
- If file contains tagged NL blocks, translates them before startup.
- Without `--file`, starts a scaffold server that returns a health response.

## Dev Shell Commands

- `/help`
- `/status`
- `/open`
- `/restart`
- `/hotfix-js <code>`
- `/hotfix-nl <prompt>`
- `/stop`
- `/start`
- `/quit`

## Hotfix Flow

- `/hotfix-js` applies explicit code edits and restarts daemon.
- `/hotfix-nl` sends pseudocode through provider translation, validates policy, and restarts daemon.

## Browser Open

- `--open` opens URL immediately.
- Otherwise Beeno asks for confirmation.
