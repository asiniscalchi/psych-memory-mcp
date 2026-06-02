# psych-memory-mcp

A minimal MCP server that fronts an internal
[`mcp-memory-service`](https://github.com/doobidoo/mcp-memory-service) and
exposes **only explicit epistemic tools**. There is deliberately no generic
`store_memory` / `save_memory` / `remember` tool: every memory must be stored
through a typed, validated tool so its epistemic status is never ambiguous.

## Tools

| Tool | Purpose |
|------|---------|
| `store_journal_fact` | Store a journal-derived fact anchored to its raw Froid source excerpt. |

`store_journal_fact` stores the verbatim `source_excerpt` as the memory content
(not the normalized statement), records every fact as `epistemic_status:
journal_reported`, and derives a deterministic, source-anchored `fact_id` (over
`entry_id` + char offsets + `sha256(source_excerpt)`) that is emitted both in
metadata and as a `fact_id:<id>` tag. The `fact_id` is intentionally stable
across re-normalization and `fact_type` correction. Validation failures come
back as structured tool errors (`{ ok: false, error_code, message }`).

## Architecture

```
assistant / 1MCP gateway
        │  http://<host>:10006/mcp   (streamable-HTTP)
        ▼
┌──────────────────┐   REST: POST/GET /api/memories   ┌────────────┐
│ psych-memory-mcp │ ───────────────────────────────▶ │   memory   │
│  (the only door) │                                   │ (internal) │
└──────────────────┘                                   └────────────┘
```

* The wrapper speaks **streamable-HTTP** at `/mcp`, the same shape the 1MCP
  gateway uses for every other service.
* The memory-service has **no published port** — it is reachable only on the
  internal compose network, so the assistant can never bypass the wrapper.

## Backend adapter

All persistence goes through the `MemoryBackend` trait
(`src/backend/mod.rs`). Two implementations:

| Implementation          | Used for | Transport |
|-------------------------|----------|-----------|
| `ReqwestMemoryBackend`  | runtime  | mcp-memory-service HTTP REST (`POST /api/memories`, `GET /api/memories/{content_hash}`, `GET /api/health`) |
| `FakeMemoryBackend`     | tests    | in-memory map |

The memory-service identifies records by **`content_hash`** (a SHA-256 of the
content), which the wrapper uses as the stable memory id. Storing identical
content is therefore idempotent.

> **Note:** memory_type `fact` is not in the service's default ontology and is
> silently downgraded to `observation` unless the service is started with
> `MCP_CUSTOM_MEMORY_TYPES='{"fact": []}'` (set in `docker-compose.yml`).

## Configuration

| Env var           | Default              | Meaning |
|-------------------|----------------------|---------|
| `MEMORY_BASE_URL` | `http://127.0.0.1:8000` | Base URL of the memory-service REST API |
| `HTTP_BIND`       | `0.0.0.0:8000`       | Address the wrapper's `/mcp` endpoint binds to |

## Build, run, test

```bash
cargo build
cargo test                 # unit tests (no service required)

# full stack
docker compose up -d --build
#   wrapper MCP endpoint:  http://127.0.0.1:10006/mcp
#   memory-service:        internal only

# transport round-trip against a running service (otherwise ignored):
MEMORY_BASE_URL=http://127.0.0.1:8000 cargo test --test roundtrip -- --ignored
```
