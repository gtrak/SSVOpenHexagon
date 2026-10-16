//! The `Online/DatabaseRecords.hpp` port: the database record structs.
//!
//! | Rust | C++ (`include/SSVOpenHexagon/Online/DatabaseRecords.hpp`) |
//! |---|---|
//! | [`User`] | `User` (`:14-20`) |
//! | [`LoginToken`] | `LoginToken` (`:22-28`) |
//! | [`Score`] | `Score` (`:30-37`) |
//! | [`ProcessedScore`] | `ProcessedScore` (`:39-45`, "not stored in database" — the wire row type; re-exported by `shared.rs` for the packet variants) |

/// C++ `hg::Database::User` (`DatabaseRecords.hpp:14-20`) — the `users`
/// table row. `passwordHash` is the C++ `std::vector<char>` (the raw
/// 32-byte `sodiumHash` digest — a BLOB column, not TEXT).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    /// C++ `id` — `INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT`. The C++
    /// caller leaves it default-initialized (`0`); sqlite_orm's plain
    /// `insert` EXCLUDES the primary-key column from the INSERT statement
    /// (`sqlite_orm.h:16137-16152`), so the autoincrement assigns it and
    /// `add_user` returns the assigned row.
    pub id: u32,
    /// C++ `steamId` — `INTEGER NOT NULL UNIQUE`.
    pub steam_id: u64,
    /// C++ `name` — `TEXT NOT NULL`.
    pub name: String,
    /// C++ `passwordHash` — `BLOB NOT NULL`.
    pub password_hash: Vec<u8>,
}

/// C++ `hg::Database::LoginToken` (`DatabaseRecords.hpp:22-28`) — the
/// `loginTokens` table row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoginToken {
    /// C++ `id` — autoincrement row; the C++ caller leaves it at `0` and
    /// sqlite_orm's plain `insert` excludes the PK column, so the
    /// autoincrement assigns it (`add_login_token` returns it).
    pub id: u32,
    /// C++ `userId` — `INTEGER NOT NULL UNIQUE`.
    pub user_id: u32,
    /// C++ `timestamp` — `Utils::nowTimestamp()` (epoch seconds,
    /// `Timestamp.cpp:19-22`).
    pub timestamp: u64,
    /// C++ `token` — `randomUInt64()` (`Sodium.cpp:76-79`).
    pub token: u64,
}

/// C++ `hg::Database::Score` (`DatabaseRecords.hpp:30-37`) — the `scores`
/// table row.
#[derive(Debug, Clone, PartialEq)]
pub struct Score {
    /// C++ `id` — autoincrement row; `add_score` reuses the existing row's
    /// id on the update path (`Database.cpp:317-319`).
    pub id: u32,
    /// C++ `levelValidator` — `TEXT NOT NULL`.
    pub level_validator: String,
    /// C++ `timestamp` — epoch seconds of the score (`Utils::nowTimestamp()`).
    pub timestamp: u64,
    /// C++ `userSteamId` — `INTEGER NOT NULL` (the steam id, NOT the `users`
    /// row id — which is why `remove_user` never orphans scores).
    pub user_steam_id: u64,
    /// C++ `value` — `REAL NOT NULL`.
    pub value: f64,
}

/// C++ `hg::Database::ProcessedScore` (`DatabaseRecords.hpp:39-45`) — the
/// score row the top-scores queries select (user name joined in) and the
/// `STCPTopScores`/`STCPOwnScore`/`STCPTopScoresAndOwnScore` packets carry.
/// "Not stored in database" (C++ comment, `:39`): it is built by
/// `get_top_scores`/`get_score` (the joined `users.name` + `scores.*`
/// projection) and travels over the wire — `crate::shared` re-exports this
/// definition for the packet types (one definition, both C++ namespaces).
#[derive(Debug, Clone, PartialEq)]
pub struct ProcessedScore {
    /// C++ `position` — the 0-based rank within the ranked result set
    /// (`Database.cpp:262-280`, `335-368`).
    pub position: u32,
    /// C++ `userName` — the joined `users.name`.
    pub user_name: String,
    /// C++ `scoreTimestamp` — the `scores.timestamp` column.
    pub score_timestamp: u64,
    /// C++ `scoreValue` — the `scores.value` column.
    pub score_value: f64,
}
