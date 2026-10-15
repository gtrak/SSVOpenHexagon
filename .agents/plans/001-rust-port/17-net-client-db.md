# Issue 17 — `oh-net` client + LeaderboardCache + SQLite database

## Objective

Port `HexagonClient` (992 lines), `LeaderboardCache`, and the server-side
`Online/Database` (SQLite schema + queries) so the Rust client can talk to the
existing C++ server, and the Rust database reads/writes the same SQLite file.

## Key decisions

- SQLite (D-004 entry): `rusqlite` with the `bundled` feature; hand-write the SQL
  from `Database.cpp` (sqlite_orm's generated SQL is the reference — transcribe the
  schema and queries).
- Client threading: the C++ client uses a background thread + queues; reproduce
  the same model (or a documented equivalent that preserves ordering/latency).
- `LeaderboardCache`: port the cache semantics (what is cached, staleness, invalidation).

## Files

| File | Change |
|---|---|
| `crates/oh-net/src/{client,leaderboard_cache,database,db_records}.rs` | new |
| `crates/oh-net/tests/database.rs` | new — schema + query tests |
| `.agents/skills/lib-sqlite.md` | new — the schema and query set from real source |

## Steps

1. Port the DB schema (tables, columns, types, indices) and every query from
   `Database.cpp`, using `rusqlite`.
2. Port `DatabaseRecords` (`ProcessedScore` etc.).
3. Port `LeaderboardCache`.
4. Port `HexagonClient`: connection, handshake, login/register, score submission,
   top-scores requests, replay upload, threading/queues.
5. Port the client-facing packet handling from `Shared.cpp` that issue 16 did not
   cover (session state).

## Verification

- DB test: create a DB, run the Rust schema, run the C++ schema (via a probe or a
  committed SQL dump), and assert the schemas are identical (`.schema` diff).
- Query tests: insert rows and assert `select` results match the C++ queries.
- A loopback test: Rust client ↔ Rust server (issue 18) exchange a full
  register/login/score flow.
- `tools/gate.sh` green.

## Notes / risks

- Cross-compatibility with the real C++ server requires issue 16's packet encoding
  to be exactly right; if the live server is unreachable, pin with recorded
  vectors instead.
