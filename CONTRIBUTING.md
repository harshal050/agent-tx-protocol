# Contributing to AgentTx

Thanks for helping improve AgentTx! The full guide lives in the docs: **[docs/contributing.md](docs/contributing.md)** (also published on the website).

## Quick setup

Prerequisites: Rust 1.88+, a C++17 toolchain with `libclang` (RocksDB builds from source), Node.js 22.12+ and pnpm 11.

```bash
git clone https://github.com/harshal050/agent-tx-protocol.git
cd agent-tx-protocol
pnpm install
```

| Command | What it does |
|---|---|
| `pnpm dev` | Documentation site at http://localhost:3000 (reads `docs/` locally) |
| `pnpm test` | Rust and TypeScript tests |
| `pnpm lint` | `cargo fmt --check`, clippy and ESLint |
| `pnpm typecheck` | TypeScript across the workspace |
| `pnpm build` | Production build of the site |
| `pnpm bench:quick` | Quick benchmark run |

## Pull requests

- Open an issue first for large changes.
- Keep PRs focused, add tests, and update `docs/` when behavior changes.
- Use [Conventional Commits](https://www.conventionalcommits.org/) (`feat(engine): …`, `fix(parser): …`, `docs: …`).
- Make sure `pnpm lint`, `pnpm typecheck` and `pnpm test` pass.

By contributing you agree that your contributions are licensed under the [Apache License 2.0](LICENSE) and that you will follow the [Code of Conduct](CODE_OF_CONDUCT.md).
