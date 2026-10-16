//! The `Online/Database.cpp` port (391 lines): the server's SQLite database —
//! schema + every query.
//!
//! ## The substitution (D-004)
//!
//! The C++ uses **sqlite_orm** (vendored `public/sqlite_orm/sqlite_orm.h`)
//! over the vendored SQLite C API (`public/sqlite/`, 3.36.0 amalgamation),
//! against the hardcoded relative file `ohdb.sqlite`
//! (`Database.cpp:44`). The Rust port uses **`rusqlite` with its `bundled`
//! feature** (DECISIONS.md D-004 SQLite entry): the schema and every query
//! are hand-transcribed from `Database.cpp` (sqlite_orm's *generated* SQL is
//! the reference), executed against the same `ohdb.sqlite` file format, so
//! a Rust `Database` and the C++ server read/write one and the same file.
//!
//! `rusqlite`'s `bundled` feature compiles the SQLite C engine from source
//! (via `libsqlite3-sys`), matching the `libsodium-sys` build-from-source
//! pattern and removing any system-sqlite dev dependency.
//!
//! ## Schema ground truth
//!
//! `tools/oracle/db_probe.cpp` compiles the REAL `Database.cpp` (vendored
//! sqlite_orm + vendored sqlite3.c) and dumps `sqlite_master` — the `.schema`
//! output. The DDL below is byte-identical to that dump, and
//! `tests/database.rs` asserts it. Two quirks of the C++ generation are
//! preserved:
//!
//! - identifiers are double-quoted and each column definition ends in
//!   `NOT NULL ` (trailing space) before the `, ` separator — the probe's
//!   exact strings;
//! - the tables are created in **reverse** `make_table` order (`scores`,
//!   `loginTokens`, `users`): sqlite_orm's `sync_schema` iterates the db
//!   objects tuple reversed (`sqlite_orm.h:18005-18013`,
//!   `iterate_tuple<true>`), which is why `sqlite_master`'s rowid order is
//!   `scores`, `sqlite_sequence`, `loginTokens`, `users` (the internal
//!   `sqlite_sequence` table is created by the engine itself when the first
//!   AUTOINCREMENT table is created — between `scores` and `loginTokens`).
//!
//! ## C++→Rust map
//!
//! | Rust | C++ (`Online/Database.cpp`) |
//! |---|---|
//! | [`Database::open`] | `Impl::makeStorage` + `getStorage` (`:40-79`) |
//! | [`Database::add_user`] | `addUser` (`:84-89`) |
//! | [`Database::remove_user`] | `removeUser` (`:92-97`) |
//! | [`Database::dump_users`] | `dumpUsers` (`:99-111`) |
//! | [`Database::any_user_with_steam_id`] | `anyUserWithSteamId` (`:113-116`) |
//! | [`Database::any_user_with_name`] | `anyUserWithName` (`:118-126`) |
//! | [`Database::get_user_with_steam_id_and_name`] | `getUserWithSteamIdAndName` (`:128-151`) |
//! | [`Database::remove_all_login_tokens_for_user`] | `removeAllLoginTokensForUser` (`:154-159`) |
//! | [`Database::add_login_token`] | `addLoginToken` (`:162-167`) |
//! | [`Database::get_all_users_with_steam_id`] | `getAllUsersWithSteamId` (`:190-198`) |
//! | [`Database::get_user_with_steam_id`] | `getUserWithSteamId` (`:181-201`) |
//! | [`Database::get_all_stale_login_tokens`] | `getAllStaleLoginTokens` (`:214-225`) |
//! | [`Database::remove_all_stale_login_tokens`] | `removeAllStaleLoginTokens` (`:228-236`) |
//! | [`Database::get_top_scores`] | `getTopScores` (`:239-266`) |
//! | [`Database::is_login_token_valid`] | `isLoginTokenValid` (`:269-282`) |
//! | [`Database::add_score`] | `addScore` (`:284-321`) |
//! | [`Database::get_score`] | `getScore` (`:324-357`) |
//! | [`Database::execute`] | `execute` (`:360-389`) |
//!
//! The C++ free functions operate on a process-wide singleton
//! (`getStorage`, `:73-77`); Rust owns the connection per [`Database`]
//! value — same behavior, no global state. Every method that reads the
//! wall clock (the stale-token predicates) is split into a pure
//! time-taking function (the C++ file-local
//! `isLoginTokenTimestampValid`, `:227-232`) so tests can pin the clock;
//! the public methods call it with `SystemTime::now()`, exactly where the
//! C++ takes `SCClock::now()`.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Batch, Connection, OptionalExtension};

use crate::db_records::{LoginToken, ProcessedScore, Score, User};

/// C++ `constexpr int tokenValiditySeconds = 3600` (`Database.cpp:204`).
pub const TOKEN_VALIDITY_SECONDS: u64 = 3600;

/// The C++ storage's tables (the `make_table` calls in `makeStorage`,
/// `Database.cpp:46-68`). The DDL strings are the sqlite_orm-generated
/// CREATE statements, byte-pinned by `tools/oracle/db_probe.cpp`.
const TABLE_SCORES_DDL: &str = "CREATE TABLE \"scores\" ( \"id\" INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL , \"levelValidator\" TEXT NOT NULL , \"timestamp\" INTEGER NOT NULL , \"userSteamId\" INTEGER NOT NULL , \"value\" REAL NOT NULL )";
const TABLE_LOGIN_TOKENS_DDL: &str = "CREATE TABLE \"loginTokens\" ( \"id\" INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL , \"userId\" INTEGER UNIQUE NOT NULL , \"timestamp\" INTEGER NOT NULL , \"token\" INTEGER NOT NULL )";
const TABLE_USERS_DDL: &str = "CREATE TABLE \"users\" ( \"id\" INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL , \"steamId\" INTEGER UNIQUE NOT NULL , \"name\" TEXT NOT NULL , \"passwordHash\" BLOB NOT NULL )";

/// The C++ `hg::Database` storage: an open SQLite connection on the
/// `ohdb.sqlite`-format file with the schema synced.
#[derive(Debug)]
pub struct Database {
    conn: Connection,
}

impl Database {
    /// C++ `Impl::makeStorage()` (`Database.cpp:40-73`):
    /// `make_storage("ohdb.sqlite", ...)` (`:44`) +
    /// `sync_schema(true /* preserve */)` (`:70`).
    ///
    /// The C++ hardcodes the relative name `ohdb.sqlite` (`:44`) and a
    /// process-wide singleton (`getStorage`, `:76-79`); the port takes the
    /// path from the
    /// composition (the server process) instead. Opening a `Database`
    /// creates the file and (on a fresh file) the three tables, exactly as
    /// `sync_schema(true)` does: each missing table is created with its
    /// pinned DDL; existing in-sync tables are left alone.
    pub fn open(path: &Path) -> Result<Database, rusqlite::Error> {
        let conn = Connection::open(path)?;
        Self::sync_schema(&conn)?;
        Ok(Database { conn })
    }

    /// The C++ `sync_schema(true)` pass (`Database.cpp:70`), transcribed: create
    /// each missing table with its exact sqlite_orm DDL, in the C++
    /// creation order (reverse `make_table` order — see the module docs).
    fn sync_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
        // C++ `table_exists` (`sqlite_orm.h:13845-13851`):
        // `SELECT COUNT(*) FROM sqlite_master WHERE type = "table" AND name = ?`.
        for (table, ddl) in [
            // Reverse make_table order: sync_schema iterates the db-objects
            // tuple reversed (sqlite_orm.h:18005-18013).
            ("scores", TABLE_SCORES_DDL),
            ("loginTokens", TABLE_LOGIN_TOKENS_DDL),
            ("users", TABLE_USERS_DDL),
        ] {
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?",
                params![table],
                |row| row.get(0),
            )?;
            if count == 0 {
                conn.execute_batch(ddl)?;
            }
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // users
    // ------------------------------------------------------------------

    /// C++ `addUser` (`Database.cpp:84-89`): `storage.insert(user)`. The
    /// sqlite_orm plain-`insert` serializer EXCLUDES the primary-key column
    /// (`sqlite_orm.h:16137-16152`) — so the C++ caller's default `id = 0`
    /// (`HexagonServer.cpp:958-963`) is never written; the AUTOINCREMENT
    /// assigns it. The port's INSERT mirrors that (no `id` column).
    /// The C++ logs the new id; the port returns it (the C++ `int id`,
    /// `:86`).
    pub fn add_user(&mut self, user: &User) -> Result<i64, rusqlite::Error> {
        self.conn.execute(
            "INSERT INTO \"users\" (\"steamId\", \"name\", \"passwordHash\") VALUES (?1, ?2, ?3)",
            params![bind_u64(user.steam_id), user.name, user.password_hash],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// C++ `removeUser` (`Database.cpp:92-97`): `remove<User>(id)` →
    /// `DELETE FROM "users" WHERE "id" = ?`. (Scores reference the steam
    /// id, not the row id, so no cascade exists — the C++ has none either.)
    pub fn remove_user(&mut self, id: u32) -> Result<(), rusqlite::Error> {
        self.conn
            .execute("DELETE FROM \"users\" WHERE \"id\" = ?", params![id])?;
        Ok(())
    }

    /// C++ `dumpUsers` (`Database.cpp:99-111`): `get_all<User>()` (the C++
    /// also logs each row; the port returns them).
    pub fn dump_users(&self) -> Result<Vec<User>, rusqlite::Error> {
        select_users(&self.conn, None, None)
    }

    /// C++ `anyUserWithSteamId` (`Database.cpp:113-116`):
    /// `!getAllUsersWithSteamId(steamId).empty()`.
    pub fn any_user_with_steam_id(&self, steam_id: u64) -> Result<bool, rusqlite::Error> {
        Ok(!select_users(&self.conn, Some(steam_id), None)?.is_empty())
    }

    /// C++ `anyUserWithName` (`Database.cpp:118-126`):
    /// `get_all<User>(where(name == c(&User::name)))` non-empty.
    pub fn any_user_with_name(&self, name: &str) -> Result<bool, rusqlite::Error> {
        Ok(!select_users(&self.conn, None, Some(name))?.is_empty())
    }

    /// C++ `getUserWithSteamIdAndName` (`Database.cpp:128-151`):
    /// empty → `None`; **more than one row → `None`** (the C++ logs a
    /// "Database integrity error" and returns `std::nullopt`, `:141-148`);
    /// exactly one → `Some(row)`.
    pub fn get_user_with_steam_id_and_name(
        &self,
        steam_id: u64,
        name: &str,
    ) -> Result<Option<User>, rusqlite::Error> {
        // The C++ query: `get_all<User>(where(steamId == c(&User::steamId)
        // && name == c(&User::name)))` — one WHERE, both conjuncts.
        let users: Vec<User> = self
            .conn
            .prepare(
                "SELECT \"id\", \"steamId\", \"name\", \"passwordHash\" FROM \"users\" WHERE \"steamId\" = ?1 AND \"name\" = ?2",
            )?
            .query_map(params![bind_u64(steam_id), name], row_to_user)?
            .collect::<Result<_, _>>()?;
        Ok(single_user(&users))
    }

    /// C++ `getAllUsersWithSteamId` (`Database.cpp:170-178`):
    /// `get_all<User>(where(steamId == c(&User::steamId)))` (`:173-175`) —
    /// every match (the C++ returns the whole `std::vector`).
    pub fn get_all_users_with_steam_id(&self, steam_id: u64) -> Result<Vec<User>, rusqlite::Error> {
        select_users(&self.conn, Some(steam_id), None)
    }

    /// C++ `getUserWithSteamId` (`Database.cpp:181-201`): the C++ calls
    /// `getAllUsersWithSteamId` and then empty → `std::nullopt` (`:185-188`),
    /// size > 1 → integrity error + `std::nullopt` (`:191-196`), else the
    /// single row.
    pub fn get_user_with_steam_id(&self, steam_id: u64) -> Result<Option<User>, rusqlite::Error> {
        Ok(single_user(&self.get_all_users_with_steam_id(steam_id)?))
    }

    // ------------------------------------------------------------------
    // loginTokens
    // ------------------------------------------------------------------

    /// C++ `removeAllLoginTokensForUser` (`Database.cpp:154-159`):
    /// `remove_all<LoginToken>(where(userId == c(&LoginToken::userId)))` →
    /// `DELETE FROM "loginTokens" WHERE "userId" = ?`.
    pub fn remove_all_login_tokens_for_user(
        &mut self,
        user_id: u32,
    ) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "DELETE FROM \"loginTokens\" WHERE \"userId\" = ?",
            params![user_id],
        )?;
        Ok(())
    }

    /// C++ `addLoginToken` (`Database.cpp:162-167`): `insert(loginToken)` —
    /// the PK column is excluded (sqlite_orm plain-`insert`,
    /// `sqlite_orm.h:16137-16152`); the C++ caller leaves `id` at `0`
    /// (`HexagonServer.cpp:1028-1033`) and the AUTOINCREMENT assigns it.
    /// Returns the assigned row, as the C++ logs it (`:164`).
    pub fn add_login_token(&mut self, login_token: &LoginToken) -> Result<i64, rusqlite::Error> {
        self.conn.execute(
            "INSERT INTO \"loginTokens\" (\"userId\", \"timestamp\", \"token\") VALUES (?1, ?2, ?3)",
            params![
                login_token.user_id,
                bind_u64(login_token.timestamp),
                bind_u64(login_token.token)
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// C++ `getAllStaleLoginTokens` (`Database.cpp:214-225`):
    /// `get_all<LoginToken>()` (ALL rows, no WHERE), then erase the VALID
    /// ones — the result is the STALE (expired-or-future-invalid) set.
    pub fn get_all_stale_login_tokens(&self) -> Result<Vec<LoginToken>, rusqlite::Error> {
        let now = SystemTime::now();
        let all = self.all_login_tokens()?;
        Ok(all
            .into_iter()
            .filter(|lt| !is_login_token_timestamp_valid(lt, now))
            .collect())
    }

    /// C++ `removeAllStaleLoginTokens` (`Database.cpp:228-236`): one
    /// `remove<LoginToken>(lt.id)` per stale token (the C++ loops, `:233-234`).
    pub fn remove_all_stale_login_tokens(&mut self) -> Result<(), rusqlite::Error> {
        for lt in self.get_all_stale_login_tokens()? {
            self.conn.execute(
                "DELETE FROM \"loginTokens\" WHERE \"id\" = ?",
                params![lt.id],
            )?;
        }
        Ok(())
    }

    /// C++ `isLoginTokenValid` (`Database.cpp:269-282`):
    /// `get_all<LoginToken>(where(token == c(&LoginToken::token)))` —
    /// empty OR more than one row → `false`; exactly one → the C++
    /// timestamp predicate on that row.
    pub fn is_login_token_valid(&self, token: u64) -> Result<bool, rusqlite::Error> {
        let tokens: Vec<LoginToken> = self
            .conn
            .prepare("SELECT \"id\", \"userId\", \"timestamp\", \"token\" FROM \"loginTokens\" WHERE \"token\" = ?")?
            .query_map(params![bind_u64(token)], row_to_login_token)?
            .collect::<Result<_, _>>()?;
        if tokens.is_empty() || tokens.len() > 1 {
            return Ok(false);
        }
        Ok(is_login_token_timestamp_valid(
            &tokens[0],
            SystemTime::now(),
        ))
    }

    fn all_login_tokens(&self) -> Result<Vec<LoginToken>, rusqlite::Error> {
        self.conn
            .prepare("SELECT \"id\", \"userId\", \"timestamp\", \"token\" FROM \"loginTokens\"")?
            .query_map([], row_to_login_token)?
            .collect()
    }

    // ------------------------------------------------------------------
    // scores
    // ------------------------------------------------------------------

    /// C++ `getTopScores` (`Database.cpp:239-266`):
    ///
    /// ```sql
    /// SELECT "users"."name", "scores"."timestamp", "scores"."value"
    /// FROM "users"
    /// JOIN "scores" ON "users"."steamId" = "scores"."userSteamId"
    /// WHERE "scores"."levelValidator" = ?
    /// ORDER BY "scores"."value" DESC
    /// LIMIT ?
    /// ```
    ///
    /// The JOIN is the sqlite_orm `join<Score>(on(c(&User::steamId) ==
    /// &Score::userSteamId))` (`:246`): scores whose `userSteamId` has no
    /// `users` row are excluded (INNER JOIN). `position` is the 0-based row
    /// index of the result (`:253-264`).
    pub fn get_top_scores(
        &self,
        top_limit: i32,
        level_validator: &str,
    ) -> Result<Vec<ProcessedScore>, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT \"users\".\"name\", \"scores\".\"timestamp\", \"scores\".\"value\" \
             FROM \"users\" JOIN \"scores\" ON \"users\".\"steamId\" = \"scores\".\"userSteamId\" \
             WHERE \"scores\".\"levelValidator\" = ?1 \
             ORDER BY \"scores\".\"value\" DESC LIMIT ?2",
        )?;
        let mut rows = stmt.query_map(params![level_validator, top_limit], |row| {
            Ok(ProcessedScore {
                position: 0, // filled in the loop, as the C++ index is
                user_name: row.get(0)?,
                score_timestamp: read_u64(row, 1)?,
                score_value: row.get(2)?,
            })
        })?;
        let mut result = Vec::new();
        for (index, row) in rows.by_ref().enumerate() {
            let mut ps = row?;
            ps.position = index as u32;
            result.push(ps);
        }
        Ok(result)
    }

    /// C++ `addScore` (`Database.cpp:284-321`): find the existing
    /// `(userSteamId, levelValidator)` score (`:296-299`); none → insert
    /// (the new `Score` has `id = 0` → autoincrement, `:302`); existing
    /// value `>= value` → **no-op** (`:310-313`); otherwise update the
    /// existing row in place (reusing its `id`, `:316-318`).
    pub fn add_score(
        &mut self,
        level_validator: &str,
        timestamp: u64,
        user_steam_id: u64,
        value: f64,
    ) -> Result<(), rusqlite::Error> {
        let existing: Option<Score> = self
            .conn
            .query_row(
                "SELECT \"id\", \"levelValidator\", \"timestamp\", \"userSteamId\", \"value\" \
                 FROM \"scores\" WHERE \"userSteamId\" = ?1 AND \"levelValidator\" = ?2",
                params![bind_u64(user_steam_id), level_validator],
                row_to_score,
            )
            .optional()?;

        match existing {
            None => {
                // The C++ `insert(score)` (Database.cpp:302) excludes the
                // PK column (sqlite_orm plain-`insert`,
                // sqlite_orm.h:16137-16152); the score's `id = 0` field
                // (Database.cpp:288-293) is never written.
                self.conn.execute(
                    "INSERT INTO \"scores\" (\"levelValidator\", \"timestamp\", \"userSteamId\", \"value\") \
                     VALUES (?1, ?2, ?3, ?4)",
                    params![level_validator, timestamp, user_steam_id, value],
                )?;
            }
            // C++: `if(existingScore.value >= value) return;` — the update
            // happens only when the new value is strictly greater.
            Some(existing) if existing.value < value => {
                self.conn.execute(
                    "UPDATE \"scores\" SET \"levelValidator\" = ?1, \"timestamp\" = ?2, \
                     \"userSteamId\" = ?3, \"value\" = ?4 WHERE \"id\" = ?5",
                    params![
                        level_validator,
                        bind_u64(timestamp),
                        bind_u64(user_steam_id),
                        value,
                        existing.id
                    ],
                )?;
            }
            Some(_) => {}
        }
        Ok(())
    }

    /// C++ `getScore` (`Database.cpp:324-357`): the same joined query as
    /// `getTopScores` but **without the LIMIT** (the C++ query at
    /// `:329-333`), then scan for the first row whose `userSteamId`
    /// matches (`:340-352`) — `position` is that
    /// row's 0-based index in the value-descending order. No matching row
    /// (or no scores at all) → `None`.
    pub fn get_score(
        &self,
        level_validator: &str,
        user_steam_id: u64,
    ) -> Result<Option<ProcessedScore>, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT \"users\".\"name\", \"scores\".\"timestamp\", \"scores\".\"value\", \
             \"scores\".\"userSteamId\" \
             FROM \"users\" JOIN \"scores\" ON \"users\".\"steamId\" = \"scores\".\"userSteamId\" \
             WHERE \"scores\".\"levelValidator\" = ?1 \
             ORDER BY \"scores\".\"value\" DESC",
        )?;
        let mut rows = stmt.query_map(params![level_validator], |row| {
            Ok((
                row.get::<_, String>(0)?,
                read_u64(row, 1)?,
                row.get::<_, f64>(2)?,
                read_u64(row, 3)?,
            ))
        })?;
        for (index, row) in rows.by_ref().enumerate() {
            let (user_name, score_timestamp, score_value, steam_id) = row?;
            if steam_id == user_steam_id {
                return Ok(Some(ProcessedScore {
                    position: index as u32,
                    user_name,
                    score_timestamp,
                    score_value,
                }));
            }
        }
        Ok(None)
    }

    // ------------------------------------------------------------------
    // raw passthrough
    // ------------------------------------------------------------------

    /// The `sqlite_master` DDL in creation (rowid) order — the `sqlite3
    /// ohdb.sqlite .schema` content. No C++ equivalent (the C++ side
    /// inspects the file with the sqlite3 CLI); this is the verification
    /// surface for the schema pin (`tests/database.rs` diffs it against
    /// the `tools/oracle/db_probe.cpp` output) and a composition
    /// convenience for the server tools (issues 18/19).
    pub fn schema_dump(&self) -> Result<Vec<String>, rusqlite::Error> {
        let mut stmt = self
            .conn
            .prepare("SELECT sql FROM sqlite_master WHERE sql IS NOT NULL ORDER BY rowid")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        rows.collect()
    }

    /// C++ `execute` (`Database.cpp:360-389`): run `query` through
    /// `sqlite3_exec` (`:380`) with a row callback that prints every
    /// result row (each cell as `%s,\t`, then `\n`, `:362-377`); on
    /// success return `Ok(())`, on failure `Err` (the SQLite error text,
    /// `:381-385`).
    ///
    /// `sqlite3_exec` runs EVERY statement in the string (the server's
    /// `db exec` admin command passes multi-word queries, although that
    /// command is currently `#if 0` — dead in the shipped C++ build,
    /// `HexagonServer.cpp:384-423`); the port walks the statements the
    /// same way with `rusqlite::Batch` (the same `sqlite3_prepare`-plus-tail
    /// algorithm as `sqlite3_exec`'s loop, `rusqlite/src/lib.rs:1075-1096`).
    /// Statements that return rows print each row (the C++ callback); DML
    /// statements execute. The printed cell text is rusqlite's native
    /// rendering (decimal integers, shortest-real, raw text, lossy-UTF-8
    /// blobs, `(null)` for NULL) — a rendering equivalent of the C++
    /// `printf("%s,\t")` callback, not byte-pinned (a server-console
    /// diagnostic, never wire data). The error text is rusqlite's
    /// `Error` rendering of the engine's message (the C++ returns the raw
    /// `sqlite3_errmsg` text, `Database.cpp:381-385`).
    pub fn execute(&self, query: &str) -> Result<(), String> {
        let mut batch = Batch::new(&self.conn, query);
        while let Some(mut stmt) = batch.next().map_err(|e| e.to_string())? {
            let n_cols = stmt.column_count();
            if n_cols > 0 {
                // A row-returning statement: run it and print every row,
                // exactly the C++ `sqlite3_exec` callback's job.
                let rows = stmt
                    .query_map([], |row| {
                        (0..n_cols)
                            .map(|i| row.get_ref(i))
                            .map(|r| r.map(cell_to_text))
                            .collect::<Result<Vec<_>, _>>()
                    })
                    .map_err(|e| e.to_string())?;
                for row in rows {
                    let cells = row.map_err(|e| e.to_string())?;
                    for cell in cells {
                        print!("{},\t", cell);
                    }
                    println!();
                }
            } else {
                // DML: the C++ callback is never invoked for it.
                stmt.execute([]).map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }
}

/// C++ file-local `isLoginTokenTimestampValid` (`Database.cpp:206-212`):
/// `(now - toTimepoint(lt.timestamp)) < std::chrono::seconds(3600)` with
/// `now = SCClock::now()` (the system clock, epoch seconds —
/// `Timestamp.cpp:12-26`).
///
/// The C++ compares SIGNED durations: a token timestamp in the future
/// yields a negative difference, which is `< 3600 s` — i.e. a future-
/// timestamped token counts as VALID. The signed arithmetic below
/// reproduces that exactly.
pub fn is_login_token_timestamp_valid(lt: &LoginToken, now: SystemTime) -> bool {
    let now_secs = now
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i128)
        .unwrap_or(0);
    now_secs - i128::from(lt.timestamp) < i128::from(TOKEN_VALIDITY_SECONDS)
}

fn row_to_user(row: &rusqlite::Row) -> rusqlite::Result<User> {
    Ok(User {
        id: row.get(0)?,
        steam_id: read_u64(row, 1)?,
        name: row.get(2)?,
        password_hash: row.get(3)?,
    })
}

fn row_to_login_token(row: &rusqlite::Row) -> rusqlite::Result<LoginToken> {
    Ok(LoginToken {
        id: row.get(0)?,
        user_id: row.get(1)?,
        timestamp: read_u64(row, 2)?,
        token: read_u64(row, 3)?,
    })
}

fn row_to_score(row: &rusqlite::Row) -> rusqlite::Result<Score> {
    Ok(Score {
        id: row.get(0)?,
        level_validator: row.get(1)?,
        timestamp: read_u64(row, 2)?,
        user_steam_id: read_u64(row, 3)?,
        value: row.get(4)?,
    })
}

/// C++ `select_users` — the shared `get_all<User>` WHERE shape:
/// `SELECT "id", "steamId", "name", "passwordHash" FROM "users"` plus the
/// caller's conjuncts (`Database.cpp:133-135`, `173-175`).
const USERS_SELECT: &str = "SELECT \"id\", \"steamId\", \"name\", \"passwordHash\" FROM \"users\"";

fn select_users(
    conn: &Connection,
    steam_id: Option<u64>,
    name: Option<&str>,
) -> Result<Vec<User>, rusqlite::Error> {
    match (steam_id, name) {
        (Some(sid), Some(n)) => conn
            .prepare(&format!(
                "{USERS_SELECT} WHERE \"steamId\" = ?1 AND \"name\" = ?2"
            ))?
            .query_map(params![bind_u64(sid), n], row_to_user)?
            .collect(),
        (Some(sid), None) => conn
            .prepare(&format!("{USERS_SELECT} WHERE \"steamId\" = ?"))?
            .query_map(params![bind_u64(sid)], row_to_user)?
            .collect(),
        (None, Some(n)) => conn
            .prepare(&format!("{USERS_SELECT} WHERE \"name\" = ?"))?
            .query_map(params![n], row_to_user)?
            .collect(),
        (None, None) => conn
            .prepare(USERS_SELECT)?
            .query_map([], row_to_user)?
            .collect(),
    }
}

/// C++ `query.empty() → nullopt; query.size() > 1 → nullopt (integrity
/// error); SSVOH_ASSERT(size == 1) → query[0]` (`Database.cpp:137-150`,
/// `185-200`).
/// C++'s `sqlite_orm` binds EVERY integer with
/// `sqlite3_bind_int64(stmt, i, static_cast<int64_t>(val))` and reads back
/// with `static_cast<T>(sqlite3_column_int64(...))` — so a `u64` value is
/// stored in the `INTEGER` cell as its SIGNED `i64` bit pattern (a `u64`
/// with the top bit set round-trips through a negative cell). rusqlite
/// refuses to bind a `u64` > `i64::MAX` at all ("out of range integral
/// type conversion attempted"), so the port performs the same bit-cast:
/// bind `x as i64`, read `i64 as u64`. (The C++'s `randomUInt64` login
/// tokens are full-range — this is load-bearing for
/// `add_login_token`/`is_login_token_valid`.)
fn bind_u64(v: u64) -> i64 {
    v as i64
}

fn read_u64(row: &rusqlite::Row, idx: usize) -> rusqlite::Result<u64> {
    Ok(row.get::<_, i64>(idx)? as u64)
}

fn single_user(users: &[User]) -> Option<User> {
    if users.is_empty() {
        return None;
    }
    if users.len() > 1 {
        // C++: SSVOH_DLOG_ERROR "Database integrity error, multiple users
        // with same steamId '...' and name '...'" (or "with same steamId")
        // and returns std::nullopt (`:141-148`, `:191-196`) — the port
        // returns None the same way (the log line is server-side
        // diagnostic output). (In practice the UNIQUE constraints make >1
        // rows unreachable.)
        return None;
    }
    Some(users[0].clone())
}

/// The `execute` cell rendering: the C++ callback's `printf("%s", argv[i])`
/// (NULL cells print as glibc's `(null)`).
fn cell_to_text(cell: rusqlite::types::ValueRef<'_>) -> String {
    use rusqlite::types::ValueRef;
    match cell {
        ValueRef::Null => "(null)".to_string(),
        ValueRef::Integer(n) => n.to_string(),
        ValueRef::Real(f) => f.to_string(),
        ValueRef::Text(t) => String::from_utf8_lossy(t).into_owned(),
        ValueRef::Blob(b) => String::from_utf8_lossy(b).into_owned(),
    }
}
