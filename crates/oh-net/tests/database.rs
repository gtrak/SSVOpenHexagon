//! The `Online/Database` port tests: the schema (diffed against the C++
//! sqlite_orm `.schema` output pinned by `tools/oracle/db_probe.cpp`) and
//! the query round-trips (the C++ free functions' observable behavior).
//!
//! The C++ reference is `src/SSVOpenHexagon/Online/Database.cpp` (391
//! lines) on the vendored sqlite_orm + SQLite 3.36.0; every test cites the
//! C++ function it pins.

use std::time::{SystemTime, UNIX_EPOCH};

use oh_net::database::{is_login_token_timestamp_valid, Database, TOKEN_VALIDITY_SECONDS};
use oh_net::db_records::{LoginToken, User};

fn now_epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// A fresh on-disk DB in a unique temp file (the C++ `ohdb.sqlite` is a
/// plain file the storage creates on open, `Database.cpp:48`).
fn open_temp_db() -> (Database, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = Database::open(&dir.path().join("ohdb.sqlite")).expect("open");
    (db, dir)
}

fn user(steam_id: u64, name: &str) -> User {
    User {
        id: 0, // the C++ caller leaves it default-initialized (HexagonServer.cpp:958-963)
        steam_id,
        name: name.to_string(),
        password_hash: vec![0xAB, 0xCD, 0xEF, 0x12],
    }
}

// ---------------------------------------------------------------------------
// Schema (the `.schema` diff)
// ---------------------------------------------------------------------------

/// The C++ `sqlite_master` content after `sync_schema(true)` on a fresh DB —
/// the `tools/oracle/db_probe.cpp` output (real `Database.cpp` + vendored
/// sqlite_orm + vendored sqlite3.c 3.36.0), in rowid (creation) order.
/// Note the quirks the Rust schema must reproduce: the double-quoted
/// identifiers, the trailing space after each `NOT NULL` before the `, `
/// separator, the `sqlite_sequence` internal table (created by the engine
/// with the first AUTOINCREMENT table), and the REVERSE table order
/// (sqlite_orm's `sync_schema` iterates the db-objects tuple reversed,
/// `sqlite_orm.h:18005-18013`).
const CPP_SCHEMA: &[&str] = &[
    "CREATE TABLE \"scores\" ( \"id\" INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL , \"levelValidator\" TEXT NOT NULL , \"timestamp\" INTEGER NOT NULL , \"userSteamId\" INTEGER NOT NULL , \"value\" REAL NOT NULL )",
    "CREATE TABLE sqlite_sequence(name,seq)",
    "CREATE TABLE \"loginTokens\" ( \"id\" INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL , \"userId\" INTEGER UNIQUE NOT NULL , \"timestamp\" INTEGER NOT NULL , \"token\" INTEGER NOT NULL )",
    "CREATE TABLE \"users\" ( \"id\" INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL , \"steamId\" INTEGER UNIQUE NOT NULL , \"name\" TEXT NOT NULL , \"passwordHash\" BLOB NOT NULL )",
];

#[test]
fn schema_matches_cpp_sqlite_orm_dump() {
    // The `.schema` diff: the Rust-created DB's `sqlite_master` content
    // (creation order) must equal the C++ sqlite_orm dump, byte for byte.
    let (db, _dir) = open_temp_db();
    let rows = db.schema_dump().unwrap();
    let expected: Vec<String> = CPP_SCHEMA.iter().map(|s| s.to_string()).collect();
    assert_eq!(
        rows, expected,
        "Rust schema must match the C++ sqlite_orm .schema output exactly"
    );
}

#[test]
fn reopening_an_in_sync_db_is_a_noop() {
    let (mut db, dir) = open_temp_db();
    db.add_user(&user(1, "alice")).unwrap();

    // C++ `sync_schema(true)` on the in-sync storage creates nothing and
    // drops nothing: the second open must see the existing row unchanged.
    let db2 = Database::open(&dir.path().join("ohdb.sqlite")).unwrap();
    let users = db2.dump_users().unwrap();
    assert_eq!(users.len(), 1);
    assert_eq!(users[0].name, "alice");
}

#[test]
fn open_creates_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ohdb.sqlite");
    assert!(!path.exists());
    let _db = Database::open(&path).unwrap();
    assert!(path.exists());
}

// ---------------------------------------------------------------------------
// users
// ---------------------------------------------------------------------------

#[test]
fn add_user_assigns_and_round_trips() {
    // C++ `addUser` (Database.cpp:102-111) + `dumpUsers` (:119-131).
    let (mut db, _dir) = open_temp_db();
    let id1 = db.add_user(&user(7001, "alice")).unwrap();
    let id2 = db.add_user(&user(7002, "bob")).unwrap();
    assert_eq!(id2, id1 + 1, "autoincrement assigns sequential rows");

    let users = db.dump_users().unwrap();
    assert_eq!(users.len(), 2);
    assert_eq!(
        users[0],
        User {
            id: id1 as u32,
            steam_id: 7001,
            name: "alice".to_string(),
            password_hash: vec![0xAB, 0xCD, 0xEF, 0x12],
        }
    );
    assert_eq!(users[1].name, "bob");
}

#[test]
fn any_user_queries() {
    // C++ `anyUserWithSteamId` (:133-136), `anyUserWithName` (:138-145).
    let (mut db, _dir) = open_temp_db();
    assert!(!db.any_user_with_steam_id(1).unwrap());
    assert!(!db.any_user_with_name("alice").unwrap());

    db.add_user(&user(1, "alice")).unwrap();
    db.add_user(&user(2, "bob")).unwrap();

    assert!(db.any_user_with_steam_id(1).unwrap());
    assert!(!db.any_user_with_steam_id(9).unwrap());
    assert!(db.any_user_with_name("bob").unwrap());
    assert!(!db.any_user_with_name("carol").unwrap());
}

#[test]
fn get_user_with_steam_id() {
    // C++ `getUserWithSteamId` (:200-224).
    let (mut db, _dir) = open_temp_db();
    assert!(db.get_user_with_steam_id(5).unwrap().is_none());

    db.add_user(&user(5, "eve")).unwrap();
    let got = db.get_user_with_steam_id(5).unwrap().unwrap();
    assert_eq!(got.name, "eve");
    assert_eq!(got.steam_id, 5);
    assert!(db.get_user_with_steam_id(6).unwrap().is_none());
}

#[test]
fn get_user_with_steam_id_and_name() {
    // C++ `getUserWithSteamIdAndName` (:147-173).
    let (mut db, _dir) = open_temp_db();
    db.add_user(&user(11, "zed")).unwrap();

    assert_eq!(
        db.get_user_with_steam_id_and_name(11, "zed")
            .unwrap()
            .map(|u| u.steam_id),
        Some(11)
    );
    // Wrong name.
    assert!(db
        .get_user_with_steam_id_and_name(11, "Zed")
        .unwrap()
        .is_none());
    // Wrong steam id (the name matches the other user).
    assert!(db
        .get_user_with_steam_id_and_name(12, "zed")
        .unwrap()
        .is_none());
}

#[test]
fn remove_user() {
    // C++ `removeUser` (:113-117).
    let (mut db, _dir) = open_temp_db();
    let id = db.add_user(&user(21, "gone")).unwrap();
    db.remove_user(id as u32).unwrap();
    assert!(db.get_user_with_steam_id(21).unwrap().is_none());
    assert!(db.dump_users().unwrap().is_empty());
}

#[test]
fn steam_id_is_unique_like_the_cpp_schema() {
    // The C++ schema gives `steamId` a UNIQUE constraint (Database.cpp:51);
    // a second insert with the same steam id must fail.
    let (mut db, _dir) = open_temp_db();
    db.add_user(&user(31, "one")).unwrap();
    let err = db.add_user(&user(31, "two")).unwrap_err();
    assert!(
        err.to_string().contains("UNIQUE"),
        "expected the UNIQUE constraint violation, got: {err}"
    );
}

// ---------------------------------------------------------------------------
// loginTokens
// ---------------------------------------------------------------------------

fn token(user_id: u32, timestamp: u64, token: u64) -> LoginToken {
    LoginToken {
        id: 0,
        user_id,
        timestamp,
        token,
    }
}

#[test]
fn token_timestamp_validity_pinned_clock() {
    // C++ `isLoginTokenTimestampValid` (Database.cpp:227-232):
    // (now - timestamp) < 3600 s, SIGNED.
    let now = UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);

    assert_eq!(TOKEN_VALIDITY_SECONDS, 3600);

    // Fresh: valid.
    assert!(is_login_token_timestamp_valid(&token(1, 1_000_000, 9), now));
    // Just inside the window: valid.
    assert!(is_login_token_timestamp_valid(
        &token(1, 1_000_000 - 3599, 9),
        now
    ));
    // Exactly at the boundary: (now - ts) == 3600 s — NOT `<`, so invalid.
    assert!(!is_login_token_timestamp_valid(
        &token(1, 1_000_000 - 3600, 9),
        now
    ));
    // Past the window: invalid.
    assert!(!is_login_token_timestamp_valid(
        &token(1, 1_000_000 - 3601, 9),
        now
    ));
    // Future timestamp: the C++ signed difference is negative < 3600 s —
    // VALID (the signed-comparison quirk, preserved).
    assert!(is_login_token_timestamp_valid(
        &token(1, 1_000_000 + 100, 9),
        now
    ));
}

#[test]
fn is_login_token_valid_real_clock() {
    // C++ `isLoginTokenValid` (Database.cpp:284-295).
    let (mut db, _dir) = open_temp_db();
    assert!(!db.is_login_token_valid(42).unwrap());

    let now = now_epoch_seconds();
    db.add_login_token(&token(1, now, 42)).unwrap();
    assert!(db.is_login_token_valid(42).unwrap());

    // Stale token (a second timestamp far in the past).
    db.add_login_token(&token(2, now - 7200, 43)).unwrap();
    assert!(!db.is_login_token_valid(43).unwrap());
}

#[test]
fn add_login_token_and_remove_all_for_user() {
    // C++ `addLoginToken` (:162-167), `removeAllLoginTokensForUser`
    // (:154-159). The schema gives `loginTokens.userId` a UNIQUE
    // constraint (Database.cpp:59) — ONE active token per user, which is
    // why the C++ login flow removes the user's tokens before adding the
    // fresh one (HexagonServer.cpp:1026-1033).
    let (mut db, _dir) = open_temp_db();
    let now = now_epoch_seconds();
    let _id1 = db.add_login_token(&token(1, now, 100)).unwrap();
    let _id3 = db.add_login_token(&token(2, now, 200)).unwrap();

    // The C++ flow: remove the user's tokens, then add the new one.
    db.remove_all_login_tokens_for_user(1).unwrap();
    let _id2 = db.add_login_token(&token(1, now + 1, 101)).unwrap();

    // User 1's old token is gone, the new one is live; user 2's survives.
    assert!(!db.is_login_token_valid(100).unwrap());
    assert!(db.is_login_token_valid(101).unwrap());
    assert!(db.is_login_token_valid(200).unwrap());

    // The UNIQUE userId constraint rejects a second live token for a
    // user (the C++ would hit the same constraint).
    let err = db.add_login_token(&token(1, now + 2, 102)).unwrap_err();
    assert!(
        err.to_string().contains("UNIQUE"),
        "expected the UNIQUE(userId) violation, got: {err}"
    );
}

#[test]
fn stale_token_queries() {
    // C++ `getAllStaleLoginTokens` (:234-245) + `removeAllStaleLoginTokens`
    // (:247-255).
    let (mut db, _dir) = open_temp_db();
    let now = now_epoch_seconds();
    db.add_login_token(&token(1, now, 10)).unwrap(); // fresh
    db.add_login_token(&token(2, now - 7200, 20)).unwrap(); // stale
    db.add_login_token(&token(3, now - 7200, 30)).unwrap(); // stale

    let stale = db.get_all_stale_login_tokens().unwrap();
    let stale_tokens: Vec<u64> = stale.iter().map(|lt| lt.token).collect();
    assert_eq!(stale_tokens, vec![20, 30]);

    db.remove_all_stale_login_tokens().unwrap();
    assert!(db.is_login_token_valid(10).unwrap());
    assert!(!db.is_login_token_valid(20).unwrap());
    assert!(!db.is_login_token_valid(30).unwrap());
}

// ---------------------------------------------------------------------------
// scores
// ---------------------------------------------------------------------------

fn setup_scores(db: &mut Database) {
    // Three users with known steam ids, one level validator.
    db.add_user(&user(1, "alice")).unwrap();
    db.add_user(&user(2, "bob")).unwrap();
    db.add_user(&user(3, "carol")).unwrap();
}

#[test]
fn add_score_insert_and_update_paths() {
    // C++ `addScore` (Database.cpp:297-333).
    let (mut db, _dir) = open_temp_db();
    setup_scores(&mut db);
    let now = now_epoch_seconds();

    // Insert path (no existing score for this user+level).
    db.add_score("lv1", now, 1, 100.0).unwrap();
    assert_eq!(db.get_top_scores(10, "lv1").unwrap().len(), 1);

    // Update path: a strictly higher value replaces the existing one
    // IN PLACE (the row count must stay 1).
    db.add_score("lv1", now + 1, 1, 150.0).unwrap();
    let top = db.get_top_scores(10, "lv1").unwrap();
    assert_eq!(top.len(), 1, "the update must not add a second row");
    assert_eq!(top[0].score_value, 150.0);
    assert_eq!(top[0].score_timestamp, now + 1);

    // No-op path: an equal or lower value leaves the row untouched.
    db.add_score("lv1", now + 2, 1, 150.0).unwrap();
    db.add_score("lv1", now + 3, 1, 50.0).unwrap();
    let top = db.get_top_scores(10, "lv1").unwrap();
    assert_eq!(top.len(), 1);
    assert_eq!(top[0].score_value, 150.0);
    assert_eq!(
        top[0].score_timestamp,
        now + 1,
        "the no-op must keep the old row"
    );
}

#[test]
fn get_top_scores_order_limit_and_join() {
    // C++ `getTopScores` (Database.cpp:257-282): value DESC, LIMIT, the
    // INNER JOIN to users (scores with no matching user row are excluded),
    // and the 0-based `position` index.
    let (mut db, _dir) = open_temp_db();
    setup_scores(&mut db);
    let now = now_epoch_seconds();

    db.add_score("lv1", now, 1, 10.0).unwrap(); // alice
    db.add_score("lv1", now, 2, 30.0).unwrap(); // bob
    db.add_score("lv1", now, 3, 20.0).unwrap(); // carol
                                                // A score whose steam id has NO users row: excluded by the JOIN.
    db.add_score("lv1", now, 999, 1000.0).unwrap();

    let top = db.get_top_scores(10, "lv1").unwrap();
    assert_eq!(
        top.iter()
            .map(|ps| (ps.position, ps.user_name.as_str(), ps.score_value))
            .collect::<Vec<_>>(),
        vec![(0, "bob", 30.0), (1, "carol", 20.0), (2, "alice", 10.0),]
    );

    // LIMIT cuts the list (the C++ server passes topScoresLimit = 6,
    // HexagonServer.cpp:814).
    let top2 = db.get_top_scores(2, "lv1").unwrap();
    assert_eq!(top2.len(), 2);
    assert_eq!(top2[0].position, 0);
    assert_eq!(top2[1].position, 1);

    // limit 0 → empty (C++ `limit(0)`).
    assert!(db.get_top_scores(0, "lv1").unwrap().is_empty());

    // Other level validators are isolated.
    assert!(db.get_top_scores(10, "lv-other").unwrap().is_empty());
}

#[test]
fn get_score_position_and_absence() {
    // C++ `getScore` (Database.cpp:335-368): the user's own ranked entry
    // (position = its index in the value-desc order), or None.
    let (mut db, _dir) = open_temp_db();
    setup_scores(&mut db);
    let now = now_epoch_seconds();

    db.add_score("lv1", now, 1, 10.0).unwrap(); // alice (rank 2)
    db.add_score("lv1", now, 2, 30.0).unwrap(); // bob (rank 1)
    db.add_score("lv1", now, 3, 20.0).unwrap(); // carol (rank 1.5)

    let ps = db.get_score("lv1", 1).unwrap().unwrap();
    assert_eq!(ps.position, 2, "alice is 3rd of 3");
    assert_eq!(ps.user_name, "alice");
    assert_eq!(ps.score_value, 10.0);
    assert_eq!(ps.score_timestamp, now);

    let ps = db.get_score("lv1", 2).unwrap().unwrap();
    assert_eq!(ps.position, 0, "bob is 1st");

    // No score for that user at that level.
    assert!(db.get_score("lv1", 999).unwrap().is_none());
    // A user with a score, but no users row: the JOIN excludes it.
    assert!(db.get_score("lv2", 3).unwrap().is_none());
}

#[test]
fn score_records_round_trip() {
    // The `Score` record itself (DatabaseRecords.hpp:30-37) is what the
    // C++ `addScore` builds; verify the columns the update path reads back
    // (existing.value, existing.id — Database.cpp:316-319).
    let (mut db, _dir) = open_temp_db();
    db.add_user(&user(41, "dan")).unwrap();
    db.add_score("lvX", 1234, 41, 7.5).unwrap();
    db.add_score("lvX", 1235, 41, 9.5).unwrap();

    // One row, the latest value, the original level validator + steam id.
    let top = db.get_top_scores(1, "lvX").unwrap();
    assert_eq!(top.len(), 1);
    assert_eq!(top[0].score_value, 9.5);
    assert_eq!(top[0].score_timestamp, 1235);
    assert_eq!(top[0].user_name, "dan");

    // A different level for the same user is an independent row.
    db.add_score("lvY", 1236, 41, 1.0).unwrap();
    assert_eq!(db.get_top_scores(10, "lvY").unwrap().len(), 1);
}

// ---------------------------------------------------------------------------
// execute (the raw passthrough)
// ---------------------------------------------------------------------------

#[test]
fn execute_select_runs_and_reports_no_error() {
    // C++ `execute` (Database.cpp:370-397): success → nullopt.
    let (mut db, _dir) = open_temp_db();
    db.add_user(&user(51, "run")).unwrap();
    // Prints the row (the C++ sqlite3_exec callback) to the test stdout.
    assert!(db
        .execute("SELECT name FROM users WHERE steamId = 51")
        .is_ok());
}

#[test]
fn execute_multi_statement_dml_runs() {
    // The C++ `sqlite3_exec` walks every statement in the string.
    let (db, _dir) = open_temp_db();
    let out = db.execute("INSERT INTO users (steamId, name, passwordHash) VALUES (61, 'a', X'00'); INSERT INTO users (steamId, name, passwordHash) VALUES (62, 'b', X'00');");
    assert!(out.is_ok(), "multi-statement exec failed: {out:?}");
    assert_eq!(db.dump_users().unwrap().len(), 2);
}

#[test]
fn execute_reports_sqlite_errors() {
    // C++: failure → Some(the error text).
    let (db, _dir) = open_temp_db();
    let err = db.execute("SELECT * FROM no_such_table").unwrap_err();
    assert!(
        err.to_lowercase().contains("no such table"),
        "expected the SQLite 'no such table' message, got: {err}"
    );
}
