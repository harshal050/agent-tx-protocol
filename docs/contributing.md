---
title: Contributing
description: Set up the monorepo, run the checks, and get a pull request merged.
---

Thanks for helping improve AgentTx. Bug reports, docs fixes, new error rules and new tools are all welcome.

## Setup

```bash
git clone https://github.com/harshal050/agent-tx-protocol.git
cd agent-tx-protocol
pnpm install
```

You need the prerequisites from [Installation](./installation.md): Rust 1.88+, a C++ toolchain, libclang, Node.js 22.12+ and pnpm.

## Everyday commands

| Command | What it does |
|---|---|
| `pnpm dev` | docs site with hot reload (reads `docs/` locally) |
| `pnpm test` | Rust tests and TypeScript tests |
| `pnpm lint` | clippy, rustfmt check, ESLint |
| `pnpm typecheck` | TypeScript across all packages |
| `pnpm build` | production build of the site |
| `pnpm bench:quick` | quick benchmark run |
| `cargo test -p agenttx-protocol` | Rust tests only |

## Pull requests

1. Open an issue first for large changes, so we can agree on the approach.
2. Keep each PR focused on one change.
3. Add tests: unit tests next to the code, scenario tests in `crates/agenttx-protocol/tests/`.
4. Update `docs/` when behavior changes. The site reads these files from `main`.
5. Make sure `pnpm lint`, `pnpm typecheck` and `pnpm test` pass.
6. Use [Conventional Commits](https://www.conventionalcommits.org/): `feat(engine): …`, `fix(parser): …`, `docs: …`.

## Good first contributions

- **Error rules:** add a `Rule` for an error format you've seen in the wild, with a test case.
- **Client examples:** Go, Java or C# examples in `docs/client-integration.md`.
- **Benchmarks:** new failure scenarios in `crates/agenttx-bench/src/sim.rs`.

## Updating benchmark results

Run the full suite on a quiet machine and commit the updated `benchmarks/results/latest.json`. Mention the hardware in the PR description.

## Code of conduct

This project follows the [Contributor Covenant](../CODE_OF_CONDUCT.md). Be kind.
