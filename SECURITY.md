# Security policy

## Supported versions

AgentTx is pre-1.0. Security fixes are made on `main` and included in the next release.

| Version | Supported |
|---|---|
| `main` / 0.1.x | ✅ |

## Reporting a vulnerability

**Please do not open a public issue for security problems.**

Report vulnerabilities privately through GitHub: **[Report a vulnerability](https://github.com/harshal050/agent-tx-protocol/security/advisories/new)**.

Include:

- the affected component (server, storage, rollback engine, built-in tools, docs site),
- steps or a proof of concept to reproduce,
- the impact you expect, and
- any suggested fix.

The maintainers will acknowledge the report, investigate, and coordinate a fix and disclosure timeline with you. Reporters are credited in the advisory unless they ask not to be.

## Deployment notes

Some risks are deployment decisions rather than vulnerabilities:

- **AgentTx has no built-in authentication or TLS.** Anyone who can reach the gRPC port can execute tools. Run it on a private network or behind an mTLS proxy or service mesh.
- The built-in `fs.*` tools are confined to `--fs-root`. Report any path that escapes the sandbox as a vulnerability.
- The documentation site's `/api/revalidate` endpoint only accepts requests signed with `GITHUB_WEBHOOK_SECRET`.
