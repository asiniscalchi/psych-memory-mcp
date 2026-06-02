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
| `store_interpretation` | Store a hypothesis grounded in one or more existing journal facts. |
| `create_pattern_seed` | Create a named observation category for a recurring dynamic. |
| `record_pattern_occurrence` | Record a dated occurrence of a pattern seed in one episode. |

`store_journal_fact` stores the verbatim `source_excerpt` as the memory content
(not the normalized statement), records every fact as `epistemic_status:
journal_reported`, and derives a deterministic, source-anchored `fact_id` (over
`entry_id` + char offsets + `sha256(source_excerpt)`) that is emitted both in
metadata and as a `fact_id:<id>` tag. The `fact_id` is intentionally stable
across re-normalization and `fact_type` correction.

`store_interpretation` stores a falsifiable hypothesis (`epistemic_status:
hypothesis`) that must reference at least one existing fact. Each
`supported_by_fact_id` is resolved via the `fact_id:<id>` tag and verified
against the fact's `metadata.fact_id`; unknown, ambiguous, or non-fact
references are rejected and nothing is stored. The deterministic
`interpretation_id` is derived from the hypothesis, type, and sorted supporting
`fact_id`s (not from confidence/status/review fields), and is emitted as both
metadata and an `interpretation_id:<id>` tag, with one `supported_by:<fact_id>`
tag per fact. Only `status: candidate` is accepted in this milestone.

`create_pattern_seed` creates a *named observation category* (`epistemic_status:
observation_category`, `status: seed`) — never a claim that the pattern is
active or that the user has it (an identity-claim guard rejects "<X> has the
pattern" / "this pattern is active"). The `pattern_id` is simply `pattern_<slug>`
(slug-only identity), emitted as `pattern_id:<id>` and `pattern_slug:<slug>`
tags with `pattern_alias:<alias>` per alias. It is idempotent by `pattern_id`:
re-creating an existing seed returns `status: already_exists` and stores
nothing (refined fields are **not** applied — updates are a future story).
Records carry no `occurrence_count`/`trend`/`intensity` — activation requires
future occurrence records.

`record_pattern_occurrence` records a *concrete dated episode*
(`epistemic_status: evidence_linked_occurrence`) of one pattern seed, grounded
in at least one existing fact and optionally linked to existing
interpretations. The pattern, every fact, and every interpretation are resolved
(by their id tags, with mandatory `metadata.<id>` exact match) before anything
is stored; the pattern seed is only read, never mutated. The deterministic
`occurrence_id` is derived from pattern/date/phase/sorted-facts/sorted-interps/
trimmed-summary (not confidence/intensity). It records a `phase` (activated,
recognized_before_action, …, not_activated, transformed) and never writes
`occurrence_count`/`trend`/activation fields — a single occurrence does not
activate the pattern.

All entity resolution (pattern seed, fact, interpretation) shares one
filter-then-count resolver: a single valid record stays usable even amid
corrupt/tag-colliding ones. This unifies — and intentionally changes — the
earlier fact-lookup corner case (one valid fact + one corrupt match: previously
`ambiguous_supporting_fact`, now resolves to the valid fact).

Validation failures from any tool come back as structured tool errors
(`{ ok: false, error_code, message }`).

> **Known backend collision risk.** The memory-service keys records by
> `content_hash = sha256(content)`. Because content is the raw excerpt (facts),
> the hypothesis (interpretations), `name — description` (pattern seeds), or the
> summary (occurrences), two entries with byte-identical content but different
> evidence collapse to one backend memory even though their
> wrapper ids differ — the last write wins. The wrapper ids, evidence tags, and
> metadata are preserved per write, but resolving this backend-level dedup is
> deferred to a later story.

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

> **Note:** the `fact`, `interpretation`, `pattern_seed`, and
> `pattern_occurrence` memory types are not in the service's default ontology
> and are silently downgraded to `observation` unless the service is started
> with `MCP_CUSTOM_MEMORY_TYPES` registering all four (set in
> `docker-compose.yml`).

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
