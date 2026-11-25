# Contributing

Maestro is early and in the open. The program runs, most of it in a first form, and [docs/status.md](docs/status.md) lists what has not been tested yet. Trying those things is as useful as writing code.

## Good first contributions

- **Run it on a Mac.** The macOS code was written from documentation and has never been compiled. Build it, run it, and open an issue saying what happened, including the compiler's errors if it does not build.
- **Open the window on Linux.** It builds and runs from the command line, but the window has not been seen.
- **Click through something untested.** Pick a line from [docs/status.md](docs/status.md) that says "not seen" or "not run", try it, and report what you found.
- **Challenge the plan.** Open an issue against anything in [docs/plan.md](docs/plan.md) that looks wrong, too costly or already solved elsewhere.
- **Vendor facts.** How a given AI coding tool reports usage, how it can be scripted, and what its terms say about being driven by another program. Link the source.
- **Fix the documentation.** If a page in [docs/](docs/README.md) does not match what the program does, say so or correct it.

## Building and testing

```
cargo run            # start it
cargo test           # the unit tests
```

See [docs/install.md](docs/install.md) for what each system needs first. Set `MAESTRO_DATA_DIR` to a scratch folder so trying things does not touch your real files.

## How to contribute

1. Open an issue first for anything larger than a typo, so the idea can be discussed before work is spent on it.
2. Keep pull requests small and say why the change is needed.
3. Say what you tested and on which system, and what you did not.
4. If you change what the program does, change its page in [docs/](docs/README.md) in the same pull request.
5. Cite sources for claims about vendor pricing, limits or terms, and give the date you checked.

Before sharing a screenshot of Maestro, check it: it shows the programs you run, the hosts they talk to and, on the Live page, the contents of agent sessions.

## AI-written contributions

Contributions written by or with an AI agent are welcome. Say so in the issue or pull request and name the agent. See [AGENTS.md](AGENTS.md).

## License

By contributing you agree that your contribution is released under the [MIT license](LICENSE).
