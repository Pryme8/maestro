# Agents

Maestro treats AI agents as crew: it finds them among the running programs, counts what they are really using, and shows what they report spending.

## Recognised by themselves

Maestro recognises these without any setup: Claude Code, the Claude app, Cursor, Codex, ChatGPT, Copilot, Gemini CLI, Aider, Windsurf, Ollama and LM Studio. That includes tools that run inside Node or Python, which are told apart by their command line. The command line is read once per process and not kept.

The rules were tried on a Windows machine running Claude Code, the Claude app, Copilot and Ollama, and with one agent on Linux. The rules for the other agents, and all of them on macOS, are untested.

## Helpers counted in

The shells, tools and scripts an agent starts are counted with it, so the processor and memory shown for an agent are what it is really using. Counting stops at any other agent it launched. **Show processes** lists them.

## Usage reports

Maestro can only show what an agent spends if the agent reports it. Agents that export OpenTelemetry metrics can report to Maestro on port 4318. The window has a **How to connect an agent** panel with the same instructions as below.

### Claude Code

Set these environment variables before starting Claude Code, or put them in its settings file under `"env"`:

```
CLAUDE_CODE_ENABLE_TELEMETRY=1
OTEL_METRICS_EXPORTER=otlp
OTEL_EXPORTER_OTLP_PROTOCOL=http/json
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4318
OTEL_METRIC_EXPORT_INTERVAL=10000
```

### Other agents

Point any OpenTelemetry metrics exporter at `http://localhost:4318` using the `http/json` protocol. Maestro reads metrics whose names end in `token.usage` and `cost.usage`.

### What the receiver keeps

- It listens on this computer only. Other computers cannot reach it.
- It keeps the token and cost figures and discards everything else in a report, including account and email fields.
- Any program on this computer can send it figures, so they are treated as the agent's own claim.
- On a subscription plan an agent reports tokens but no real bill, so the cost shown is the vendor's own estimate.

The receiver has been tested with a sample report, not yet with a real agent.

## Budgets and energy

Give an agent a daily budget in dollars and its energy bar shows what is left of it. Passing the budget puts a warning in the action log. Maestro does not stop the agent: pausing an agent automatically when its budget runs out is planned and will be opt-in.

## Quests

Name a piece of work, assign it to an agent, and Maestro shows what the agent has reported spending since the quest started. Mark it **Done** when it is finished.

## From the command line

`--agents` lists the agents found running. `--receive` runs only the usage receiver and prints totals as reports arrive.
