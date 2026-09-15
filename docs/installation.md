---
title: Installation
description: Build prerequisites, running from source, Docker, and working on the monorepo.
---

## Prerequisites

| Requirement | Version | Why |
|---|---|---|
| Rust | 1.88 or newer | The crate uses edition 2024 |
| C++17 compiler | GCC 11+ or Clang 14+ | RocksDB is compiled from source |
| libclang | any recent | `bindgen` generates the RocksDB bindings |
| Node.js + pnpm | 22.12+ / 11+ | Only for the documentation site |

You do **not** need `protoc`. The protobuf contract is compiled in pure Rust with `protox`.

### Debian / Ubuntu

```bash
sudo apt-get install -y build-essential clang libclang-dev
```

### macOS

```bash
xcode-select --install
```

> [!WARNING]
> Seeing `fatal error: 'stdbool.h' file not found` while building `librocksdb-sys`? Your libclang is installed without its resource headers. Install the matching `libclang-common-<version>-dev` package, or point bindgen at GCC's headers:
>
> ```bash
> export BINDGEN_EXTRA_CLANG_ARGS="-I$(dirname "$(gcc -print-file-name=include/stdbool.h)")"
> ```

## Build and run

```bash
cargo build --release -p agenttx-protocol
./target/release/agenttx --help
```

The binary is self-contained. RocksDB is statically linked.

## Docker

The repository ships a multi-stage `Dockerfile` that produces a slim runtime image:

```bash
docker build -t agenttx .
docker run --rm -p 50051:50051 -v agenttx-data:/data agenttx
```

See [Deploy & operate](./operations.md) for production settings.

## Working on the monorepo

The repository is a Turborepo monorepo with a Cargo workspace inside it:

```text
crates/agenttx-protocol   Rust server and library
crates/agenttx-bench      benchmark harness
apps/docs                 this website (Next.js)
packages/content          GitHub/local content loader + Markdown pipeline
packages/benchmarks       benchmark result schema and metrics
packages/ui               shared React components and charts
docs/                     documentation source (Markdown)
benchmarks/results        committed benchmark results (JSON)
```

```bash
pnpm install
pnpm dev          # docs site on http://localhost:3000
pnpm test         # Rust and TypeScript tests through Turborepo
pnpm bench        # run the benchmark suite and update benchmarks/results/latest.json
```
