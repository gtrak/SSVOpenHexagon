# Skill — SQLite + sqlite_orm (`Online/Database.*`, ported as `oh-net` `database`/`db_records`)

Written against: the real `src/SSVOpenHexagon/Online/Database.cpp` (391
lines) + `include/SSVOpenHexagon/Online/{Database,DatabaseRecords}.hpp`,
the vendored `public/sqlite_orm/sqlite_orm.h` (the header-only ORM the
C++ side uses), and the Rust port in `crates/oh-net/src/{database,
db_records}.rs` (rusqlite 0.31.0 `bundled`, DECISIONS.md D-004). The
authoritative `.schema` was captured by `tools/oracle/db_probe.cpp`
(compiles the REAL `Database.cpp` + vendored sqlite_orm against the
vendored `public/sqlite/sqlite3.c` 3.36.0 and dumps `sqlite_master`) —
committed as `tools/oracle/db_probe_schema.txt` and byte-pinned by
`crates/oh-net/tests/database.rs` (`schema_diff_matches_cpp_probe`).

## The schema (byte-exact)

The sqlite_orm `sync_schema` DDL — identifiers are **double-quoted**,
every column ends with a **trailing space before the comma** (`"name"
TEXT NOT NULL ,`), and the tables are created in **reverse** of the
`make_table` registration order (sqlite_orm iterates the db-objects
tuple backwards, `sqlite_orm.h:18005-18013`), so `sqlite_master` lists
them:

```sql
CREATE TABLE "scores" ( "id" INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL , "levelValidator" TEXT NOT NULL , "timestamp" INTEGER NOT NULL , "userSteamId" INTEGER NOT NULL , "value" REAL NOT NULL )
CREATE TABLE sqlite_sequence(name,seq)
CREATE TABLE "loginTokens" ( "id" INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL , "userId" INTEGER UNIQUE NOT NULL , "timestamp" INTEGER NOT NULL , "token" INTEGER NOT NULL )
CREATE TABLE "users" ( "id" INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL , "steamId" INTEGER UNIQUE NOT NULL , "name" TEXT NOT NULL , "passwordHash" BLOB NOT NULL )
```

(`sqlite_sequence` appears second because the first `AUTOINCREMENT`
table created — `scores` — triggers its creation.)

- `users` (`Database.cpp:44-50`): `id` AI-PK; `steamId` **UNIQUE**;
  `name` TEXT; `passwordHash` BLOB (the raw 32-byte `sodiumHash`
  digest — NOT a string/UTF-8).
- `loginTokens` (`Database.cpp:53-59`): `id` AI-PK; `userId` **UNIQUE**
  (one live token per user — the C++ server removes the user's tokens
  before adding a fresh one, `HexagonServer.cpp:1026-1033`);
  `timestamp` INTEGER; `token` INTEGER.
- `scores` (`Database.cpp:62-68`): `id` AI-PK; `levelValidator` TEXT;
  `timestamp` INTEGER; `userSteamId` INTEGER; `value` REAL.

The Rust `Database::open` (→ `Database::sync_schema`) issues these
statements verbatim (CREATE TABLE IF NOT EXISTS) in that same order.
Types: sqlite_orm maps `std::uint64_t`→`INTEGER`, `double`→`REAL`,
`std::string`→`TEXT`, and the `std::vector<uint8_t>` passwordHash→`BLOB`
(`DatabaseRecords.hpp`). The `users.passwordHash` BLOB is bound from /
read back as a Rust byte vector.

## Every query (C++ free function → SQL, all verified)

All from `Database.cpp`; the Rust methods in `crates/oh-net/src/
database.rs` carry the same citations. "PK-excluded INSERT" = the
sqlite_orm plain-`insert` serializer excludes the primary-key column
(`sqlite_orm.h:16137-16152`), so the C++ caller's default `id = 0` is
never written and AUTOINCREMENT assigns the row — the Rust INSERTs
mirror that (no `id` column).

| C++ (`Database.cpp`) | SQL (semantics) | Rust (`database.rs`) |
|---|---|---|
| `makeStorage` `:40-73` | `make_storage("ohdb.sqlite", ...)` (`:44` — the relative name is hardcoded; there is no path function) + `sync_schema(true /* preserve */)` (`:70`) | `Database::open` (the path comes from the composition instead — documented in `database.rs`; schema sync) |
| `getStorage` `:76-79` | — | the `Database` handle itself |
| `addUser` `:84-89` | PK-excluded INSERT `("steamId","name","passwordHash")`; returns new id | `add_user` (returns the assigned `id`; C++ logs it) |
| `removeUser` `:92-97` | `DELETE FROM "users" WHERE "id" = ?` | `remove_user` |
| `dumpUsers` `:99-111` | `get_all<User>()` — a plain all-rows select (no WHERE, no ORDER BY) | `dump_users` |
| `anyUserWithSteamId` `:113-116` | `WHERE "steamId" = ?` (existence — the C++ wraps `getAllUsersWithSteamId(...).empty()`) | `any_user_with_steam_id` |
| `anyUserWithName` `:118-126` | `WHERE "name" = ?` (existence) | `any_user_with_name` |
| `getUserWithSteamIdAndName` `:128-151` | `WHERE "steamId" = ? AND "name" = ?` → single-row `Option<User>` (empty→None; >1→None+assert) | `get_user_with_steam_id_and_name` |
| `removeAllLoginTokensForUser` `:154-159` | `DELETE FROM "loginTokens" WHERE "userId" = ?` | `remove_all_login_tokens_for_user` |
| `addLoginToken` `:162-167` | PK-excluded INSERT `("userId","timestamp","token")` | `add_login_token` |
| `getAllUsersWithSteamId` `:170-178` | `WHERE "steamId" = ?` (all rows) | `get_all_users_with_steam_id` |
| `getUserWithSteamId` `:181-201` | single-row `Option<User>` | `get_user_with_steam_id` |
| `tokenValiditySeconds` `:204` | `constexpr int = 3600` (ONE hour) | `TOKEN_VALIDITY_SECONDS` |
| `isLoginTokenTimestampValid` `:206-212` | `(now - timestamp) < 3600 s` (STRICT), the difference computed as **SIGNED** — a future `timestamp` yields a negative difference, which is `< 3600 s` → VALID (the port keeps the signed i128 comparison) | `is_login_token_timestamp_valid` |
| `getAllStaleLoginTokens` `:214-225` | **NOT a SQL WHERE** — `get_all<LoginToken>()` (all rows) then an in-memory `erase(remove_if(isLoginTokenTimestampValid))` (stale = NOT valid) | `get_all_stale_login_tokens` (same in-memory filter) |
| `removeAllStaleLoginTokens` `:228-236` | the stale list from above, then one `remove<LoginToken>(lt.id)` — `DELETE FROM "loginTokens" WHERE "id" = ?` — PER stale token (the C++ loops, `:232-235`) | `remove_all_stale_login_tokens` (same loop) |
| `getTopScores` `:239-266` | `SELECT "users"."name", "scores"."timestamp", "scores"."value" FROM "users" INNER JOIN "scores" ON "users"."steamId" = "scores"."userSteamId" WHERE "scores"."levelValidator" = ? ORDER BY "scores"."value" DESC LIMIT ?` — the INNER JOIN **excludes** scores whose user row was deleted (no `COALESCE`/LEFT JOIN); positions are assigned in the C++ loop (`:252-264`) | `get_top_scores` |
| `isLoginTokenValid` `:269-282` | `WHERE "token" = ?` — empty OR >1 row → false; else the timestamp check above | `is_login_token_valid` |
| `addScore` `:284-321` | `WHERE "levelValidator" = ? AND "userSteamId" = ?`; none → PK-excluded INSERT; existing `value >=` new → **no-op** (`:310-313`); else in-place UPDATE reusing the row's `id` (`:316-318` — the sqlite_orm `update` sets all non-PK columns, `sqlite_orm.h:16044-16068`) | `add_score` |
| `getScore` `:324-357` | the same JOINed query as `getTopScores` but with the `userSteamId` selected too, NO `LIMIT`, and the C++ scans the ordered rows in memory for the row with the given `userSteamId`, returning its 0-based index as `position` (`:345-355`) — `nullopt` when absent (incl. the JOIN-excluded case) | `get_score` (same in-memory scan) |
| `execute` `:360-389` | `sqlite3_exec` over a multi-statement string (walks every statement; prints select rows tab-separated; returns `Some(errText)` on the first failure) | `Database::execute` (rusqlite `Batch` for the multi-statement walk, row printing via `cell_to_text`) |

`ProcessedScore` (`DatabaseRecords.hpp:39-45`: `position`, `userName`,
`scoreTimestamp`, `scoreValue`) lives in `db_records.rs` (re-exported
from `shared.rs` — the wire path is unchanged, issue 17 extend-only).

## Traps / gotchas (all hit during the port)

1. **`id = 0` inserts are not "assign me"**. `INTEGER PRIMARY KEY` is
   the rowid; inserting `0` explicitly creates rowid 0 and a second
   insert of 0 is a `UNIQUE` violation. The C++ never hits this because
   sqlite_orm's plain `insert` excludes the PK column entirely
   (`sqlite_orm.h:16137-16152`) — the Rust INSERTs must too.
2. **`loginTokens.userId` is UNIQUE** — a user can hold exactly one live
   token; the C++ login flow relies on `removeAllLoginTokensForUser`
   first (`HexagonServer.cpp:1026-1033`).
3. **Staleness is signed and in-memory.** `isLoginTokenTimestampValid`
   is `(now - timestamp) < 3600 s` (ONE hour, strict) with the difference
   computed SIGNED — a token with a *future* timestamp is "valid" (not
   stale); the port pins that quirk. `getAllStaleLoginTokens` is NOT a
   SQL `WHERE timestamp < ?` — it selects ALL tokens and filters in
   memory, and `removeAllStaleLoginTokens` deletes per-id in a loop
   (`tests/database.rs` pins both).
4. **The top-scores JOIN is INNER** — a score whose `users` row is gone
   is silently dropped from `getTopScores` AND from `getScore`'s
   position scan (no `COALESCE`, no LEFT JOIN).
5. **`addScore` is an in-place update, not an append** — and a
   lower-or-equal score is a no-op (the existing row is returned).
6. **The DDL whitespace is load-bearing for the `.schema` diff** — the
   double quotes, the trailing spaces, and the reversed table order come
   straight from sqlite_orm and are byte-pinned; rusqlite (the bundled
   SQLite engine) accepts them unchanged.

## Probe & tests

- `tools/oracle/db_probe.cpp` — builds the C++ schema in a temp dir
  (`Database::getStorage().makeStorage(path)` semantics via the real
  `Database.cpp`), runs a couple of representative C++ queries, and
  prints `.schema` + results between `=== ohdb.sqlite schema (C++
  sqlite_orm, real Database.cpp) ===` markers. Run:
  `tools/oracle/db_probe.cpp` is compiled against
  `src/SSVOpenHexagon/Online/Database.cpp` + `public/sqlite_orm/
  sqlite_orm.h` + `public/sqlite/sqlite3.c` (see the probe's header
  comment for the exact command; the vendored `extlibs` paths come from
  the main repo when the worktree submodules are not initialized).
- `crates/oh-net/tests/database.rs` — 20 tests: the `.schema` diff
  against the committed probe dump, the no-op-on-reopen schema, and the
  per-function round-trips (users, tokens incl. the UNIQUE(userId)
  behaviour, the signed token-validity with a pinned clock, stale-token
  select/remove, the score insert/update/no-op paths, top-scores
  order/limit/JOIN exclusion, `execute` multi-statement + error
  reporting).
