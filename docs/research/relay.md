# Relay: how each tool is run without its window, and what its terms say

Desk research, read on 2026-11-24 by an AI agent. Nothing here has been run. Every source below is tagged [P]: the page itself was opened and read. No claim rests on a search snippet. Most pages show no date; dates are given where shown. This is a reading of public documents, not legal advice.

The relay Maestro plans would start a vendor's own command-line tool for one prompt, with the login already on the machine, read what it prints, and pass a short brief to the next tool. Maestro would never hold a credential or call a vendor's API itself. Two questions matter per tool: how it is run for one prompt, and whether its terms let another program do that on a subscription login.

## What Maestro does with this

| Tool | On a pay-per-use key | On a subscription login |
|---|---|---|
| Claude Code | command shown | command shown, with the caveat below |
| Codex CLI | command shown | no command: terms do not address it |
| Cursor CLI | command shown | no command: terms do not address it |
| Gemini CLI | command shown | no command: consumer plans no longer served, terms do not address it |

Where the terms do not address a launched tool on a subscription login, Maestro shows no command and says why. That is a choice to stay on the safe side of an unanswered question, not a finding that it is forbidden. The relay is a dry run in this build: it prints the plan and starts nothing.

## Claude Code (Anthropic)

**Running one prompt** ([headless](https://code.claude.com/docs/en/headless) [P], [CLI reference](https://code.claude.com/docs/en/cli-reference) [P])

- `claude -p "<prompt>"`. The prompt can also be piped in.
- `--output-format json` gives one JSON object. The final text is in `result`. It also carries `session_id` and `total_cost_usd`, which is an estimate made on the client.
- No working-directory flag was found in the flag table; the tool uses the directory it is started in.
- Permissions: from version 2.1.302, `-p` starts in manual mode, and a call that needs approval is denied unless something approves it. `--allowedTools` and `--permission-mode` grant more.
- `--model` picks the model.
- Exit code 0 on success, non-zero on failure. Some failures, such as missing sign-in, are printed as the result.
- `--bare` does not use the subscription login and needs an API key. The documents say `--bare` will become the default for `-p` in a future release, which would end subscription relays unless the flag is turned off.

**Terms** ([legal and compliance](https://code.claude.com/docs/en/legal-and-compliance) [P], [consumer terms](https://www.anthropic.com/legal/consumer-terms) [P], effective 2025-10-08)

- Using the credentials yourself is prohibited: third-party developers may not offer Claude.ai login in their own applications, route requests through plan credentials, or collect, store or pass on credentials or session tokens.
- Starting the unmodified tool is addressed, favourably: the restriction does not prevent a person signing in to the unmodified Claude Code with their own subscription, including where a platform hosts Claude Code. Products that run Claude Code must not modify it, disable a sign-in method, or pay for, resell or intermediate usage.
- Caveat: plan limits assume ordinary, individual usage, and the consumer terms bar access by automated means except through an API key or where Anthropic explicitly permits it. Whether the documented `claude -p` mode counts as that permission for a subscription login is not stated. A local program starting `claude -p` is not named either way.

## Codex CLI (OpenAI)

**Running one prompt** ([non-interactive mode](https://learn.chatgpt.com/docs/non-interactive-mode) [P], [command reference](https://learn.chatgpt.com/docs/developer-commands?surface=cli) [P], first two thirds read). OpenAI's Codex documents now redirect to learn.chatgpt.com; the redirect was followed from OpenAI's own server and the domain was not otherwise verified.

- `codex exec "<prompt>"`. Progress goes to the error stream and only the final message to standard output.
- `--json` gives one JSON event per line. The final text is the `text` of an `item.completed` event whose item is an `agent_message`. Token counts are in `turn.completed.usage`. There is no cost field.
- `--cd` sets the working directory. Outside a git repository, `--skip-git-repo-check` is needed.
- Permissions: by default it runs in a read-only sandbox. `--sandbox` takes `read-only`, `workspace-write` or `danger-full-access`.
- `--model` picks the model. Exit codes are not documented.

**Terms** ([auth](https://learn.chatgpt.com/docs/auth) [P], [terms of use](https://openai.com/policies/terms-of-use/) [P], effective 2026-01-01, [service terms](https://openai.com/policies/service-terms/) [P], updated 2026-11-13)

- `codex exec` reuses the saved sign-in, so a ChatGPT login works technically. The documents say API keys are the recommended default for automation.
- Using the credentials yourself: no Codex-specific clause was found. The general terms forbid sharing account credentials and circumventing rate limits.
- Starting the unmodified tool from another program: not addressed. The Sign in with ChatGPT terms were not read.

## Cursor CLI

**Running one prompt** ([headless](https://cursor.com/docs/cli/headless) [P], [parameters](https://cursor.com/docs/cli/reference/parameters) [P])

- `agent -p "<prompt>"`. The program is called `agent` in the current documents.
- `--output-format json`. The final text is in `result`. The rest of the JSON, including any usage figures, was not confirmed.
- `--workspace <path>` sets the working directory.
- Permissions: in this mode it writes to files in the workspace without being asked. Writes outside the workspace, shell commands that are not allowed, and web fetches are rejected unless `--force` is given.
- `--model` picks the model. Exit codes are not documented. An administrator can disable this mode for a team.

**Terms** ([authentication](https://cursor.com/docs/cli/reference/authentication) [P], [terms of service](https://cursor.com/terms-of-service) [P], updated 2026-09-03)

- Browser sign-in is recommended for people; for automation, scripts and CI the documents say to use an API key.
- Neither using the credentials nor starting the tool from another program is addressed. The acceptable use policy the terms point to was not read.

## Gemini CLI (Google)

**Running one prompt** ([repository](https://github.com/google-gemini/gemini-cli) [P]: `docs/cli/headless.md`, `docs/cli/cli-reference.md`, `README.md`)

- `gemini -p "<prompt>"`.
- `--output-format json`. The final text is in `response`, with token counts in `stats`.
- No working-directory flag; it uses the directory it is started in.
- Permissions: `--approval-mode` takes `default`, `auto_edit`, `yolo` or `plan`. What `default` does to a tool call in this mode was not confirmed.
- `--model` picks the model. Exit codes: 0 success, 1 general error, 42 input error, 53 turn limit.

**Terms** (README [P], [authentication](https://geminicli.com/docs/get-started/authentication/) [P], `docs/resources/tos-privacy.md` [P])

- Consumer access was removed: the README says free and Google AI plan access moved to Antigravity CLI on 2026-06-18. Gemini CLI now serves Code Assist licences and API keys.
- Using the credentials yourself is prohibited: reaching the services behind Gemini CLI with third-party software is named as a violation.
- Starting the unmodified tool from another program: not addressed.

## Not confirmed

- **Claude Code:** the full list of JSON fields; whether the automated-access exception covers `claude -p` on a subscription login; when `--bare` becomes the default.
- **Codex CLI:** exit codes; the last third of the command reference; the Sign in with ChatGPT terms.
- **Cursor CLI:** the JSON fields; exit codes; the acceptable use policy; how this mode is counted against a subscription.
- **Gemini CLI:** what the default approval mode does here; whether Antigravity CLI has an equivalent mode and what its terms say.
