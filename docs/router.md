# Router

The Router page holds a ledger of your AI accounts and an advisor that proposes the cheapest way to do a task with them. It works the same from the command line. It starts nothing: the relay that would run a route is not built, and only prints its plan.

## Ledger

You list your AI accounts by hand. Each one has:

- a name you choose,
- the tool, such as `claude-code`,
- the model,
- how you rate the model: light, standard or heavy,
- and one of:
  - **pay per use:** its price in dollars per million tokens, in and out, or
  - **allowance:** your estimate of how many tokens a subscription allows over a rolling window of hours.

Maestro knows no vendor's prices or limits. Every figure is yours.

What is left of an allowance is counted from [usage reports](agents.md#usage-reports) over the window, and is always shown as an estimate.

## Advisor

Give it a task's size in tokens and the rating it needs, and it proposes up to three routes:

- the cheapest single tool that fits,
- a single tool that leaves every allowance untouched,
- a draft on a cheaper model followed by a review on a stronger one.

Each route shows what it adds to your bill and what is left of each allowance. The advisor keeps a tenth of each allowance in reserve and says when a step would eat into it.

### Handoffs are costed

A two-tool split counts the tokens for the handoff brief and for the second tool reading the work again. A split is only proposed when it still saves money or you asked for a review, and small tasks are told to stay in one tool.

## Relay plan

**Show relay plan** prints, for a route, the command each step would run and the handoff brief the tools would pass. It is a dry run and starts nothing.

A command is shown only where the vendor's own documents support it: Claude Code on a subscription login or an API key, and Codex, Cursor and Gemini CLI on an API key only. The reasons are in [research/relay.md](research/relay.md). The commands come from reading each vendor's documents and have not been run.

## Limits

- **Usage is counted per tool,** not per account or model. Two accounts on the same tool share one figure. The named sign-ins on the [Live](live.md#more-than-one-account) page are not tied to ledger accounts yet.
- **Use from other devices is not seen,** nor use in a vendor's web app, so an allowance can be lower than shown.
- **The advisor cannot judge answer quality.** "Best route" means cheapest within your ratings.

## From the command line

```
maestro --ledger
maestro --account-add work claude-code opus heavy allowance 2000000 168
maestro --account-add api claude-code sonnet standard payperuse 3 15
maestro --account-remove work
maestro --advise 300000 --tier heavy --review
maestro --advise 300000 --tier heavy --review --relay-plan
```

The figures in these examples are made up. The forms are:

- `--account-add <name> <tool> <model> <light|standard|heavy> payperuse <$ in per million> <$ out per million>`
- `--account-add <name> <tool> <model> <light|standard|heavy> allowance <tokens> <window in hours>`
- `--advise <tokens> [--tier light|standard|heavy] [--review] [--relay-plan]`. The tier is standard if you leave it out.
