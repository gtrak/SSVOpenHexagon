# Audit: `Core/HexagonClient` ↔ `crates/oh-net` (client / leaderboard cache / database)

**Pair:** `src/SSVOpenHexagon/Core/HexagonClient.cpp` (992 lines, read completely) +
`include/SSVOpenHexagon/Core/HexagonClient.hpp` (206) ↔
`crates/oh-net/src/lib.rs` module tree → `client.rs` (2428), `leaderboard_cache.rs` (308),
`database.rs` (639), `db_records.rs` (82), plus the wire/codec dependencies the client calls
(`shared.rs`, `sodium.rs`, `packet.rs`, `protocol.rs` — client-relevant paths verified, the full
issue-16 codec audit is a separate lane).

**Scope note:** the audit focus (online flow, kick handling, leaderboard cache invalidation,
profile/database operations) pulls `Core/LeaderboardCache.{hpp,cpp}` and
`Online/Database.{hpp,cpp}` + `Online/DatabaseRecords.hpp` into this pair, because their Rust
counterparts live in this crate (per the `lib.rs:1-35` mapping table). The *consumer* of the
client events + cache (the C++ `MenuGame.cpp:2560-2630` event drain and `:5598-5610`
score-request gate) is a different module pair (`oh-menu` — verified to exist and to handle all
15 events incl. `Kicked` at `crates/oh-menu/src/menu_game.rs:746-758,1832`); it is **out of
scope** here, noted only to confirm this crate exposes everything the consumer needs.

**Method:** full read of both sides; mechanical function-by-function diff; every suspected
absence grep-verified; SFML fork semantics checked against the pinned fork source
(`/tmp/sfml_fork/src/SFML/Network/{TcpSocket,Packet}.cpp`); C++ line citations in the Rust
comments re-verified against the C++ file (drift found — see DIFFERENT D3).

---

## C++ behavior inventory (HexagonClient.cpp)

| Behavior | C++ (file:line) | Rust counterpart | Status |
|---|---|---|---|
| diagnostic logging (`clog`, `SSVOH_CLOG*`, `_verbose` gate) | `:29-40` infra; `_verbose{true}` `:448`; 67 call sites | none (no log dependency in `crates/oh-net/Cargo.toml`) | **MISSING (documented)** |
| `fail<Ts...>` (log + return false) | `:43-57` | plain `false` returns (log dropped) | covered by logging item |
| `initializeTicketSteamID` (poll `run_callbacks` + 50 ms sleep `:63`, `tries > 15` fail, ticket/steamId checks, sets `_ticketSteamID` `:87`) | `:55-89` | `initialize_ticket_steam_id` `client.rs:677-702` | PRESENT |
| `initializeTcpSocket` (already-connected fail `:93-96`; `setBlocking(true)` `:98`; 0.5 s connect `:102-104`; failure keeps `_socketConnected=false` `:107`; `setBlocking(false)` `:111`) | `:91-116` | `initialize_tcp_socket` `client.rs:704-719` + `StdTcpSocket::connect` `:192-207` | PRESENT |
| `sendPacketRecursive` (tries>5 fail `:119-122`; NotReady/Error/Disconnected fail; Partial → recurse) | `:118-149` | `send_packet_recursive` `client.rs:725-737` | PRESENT |
| `recvPacketRecursive` (same; Error/Disconnected → `disconnect()` `:168-178`) | `:152-188` | `recv_packet_recursive` `client.rs:740-751` | PRESENT |
| `sendPacket`/`recvPacket` wrappers | `:192-199` | `client.rs:754-763` | PRESENT |
| `sendUnencrypted<T>` (gate `_socketConnected` `:204-207`; `makeClientToServerPacket`; `sendPacket`) | `:203-212` | `send_unencrypted` `client.rs:766-773` | PRESENT |
| `sendEncrypted<T>` (gate connected `:216-219`; gate RT keys `:221-223`; `makeClientToServerEncryptedPacket` bool check `:224-228`; send) | `:215-234` | `send_encrypted` `client.rs:775-793` (infallible make — SEAM S6) | PRESENT |
| `sendHeartbeat` (CTSPHeartbeat + stamp `_lastHeartbeatTime` `:243`) | `:236-244` | `client.rs:795-802` | PRESENT |
| `sendDisconnect` / `sendPublicKey` | `:247-254` | `client.rs:804-814` | PRESENT |
| `sendRegister`/`sendLogin`/`sendLogout`/`sendDeleteAccount` | `:257-302` | `client.rs:816-846` | PRESENT |
| `sendRequestTopScores`/`sendRequestOwnScore`/`sendRequestTopScoresAndOwnScore`/`sendStartedGame`/`sendCompressedReplay`/`sendRequestServerStatus`/`sendReady` | `:304-392` | `client.rs:847-906` | PRESENT |
| `connect()` (`Connecting` `:397`; already-connected → fail no event `:398-401`; failEvent lambda = `EConnectionFailure{"Failure connecting, error <reason>"}` + `ConnectionError` `:404-412`; heartbeat `:425-428`; public key `:430-432`; `EConnectionSuccess` + `Connected` `:432-433`) | `:395-434` | `connect` `client.rs:925-952` (byte-exact error strings) | PRESENT |
| ctor (PS keys `:449`; init log incl. secret key `:455-463`; ticket fail → `InitError` `:464-470`; `connect()` `:472`) | `:437-474` | `with_socket`/`new` `client.rs:625-670` | PRESENT |
| dtor → `disconnect()` | `:475-480` | `impl Drop` `client.rs:1356-1362` | PRESENT |
| `disconnect()` (`setBlocking(true)` `:486`; (LoggedIn\|Ready)+ticket → `sendLogout` `:488-490`; (Connected\|LoggedIn\|Ready) → `sendDisconnect` `:493-495`; close `:500-501`; `Disconnected` `:505`) | `:482-505` | `disconnect` `client.rs:966-988` | PRESENT |
| `sendHeartbeatIfNecessary` (no socket → true `:509-512`; 45 s window `:514-516` [const `:511`, C++ typo `heatbeatInterval`]; fail → log + `disconnect()` `:518-526`) | `:508-530` | `client.rs:991-1004` | PRESENT |
| `receiveDataFromServer` — all 17 arms: `PInvalid` `:552-556` (fail, no disconnect); `PEncryptedMsg` `:558-561` (fail); `STCPKick` `:563-570` (**`EKicked` + `disconnect()` + true**); `STCPPublicKey` `:573-617` (store key `:586`, RT keys calc, fail → disconnect+false `:592-600`); `STCPRegistrationSuccess/Failure` `:619-633`; `STCPLoginSuccess` `:636-657` (token `:650`, name `:651`, `LoggedIn` `:653`, **immediately** `sendRequestServerStatus` `:656`); `STCPLoginFailure` `:659-666`; `STCPLogout*` `:668-682`; `STCPDeleteAccount*` `:684-700`; `STCPTopScores` `:701-711`; `STCPOwnScore` `:713-722`; `STCPTopScoresAndOwnScore` `:724-741` (top event always; own event only when present); `STCPServerStatus` `:743-773` (game mismatch → event only `:749-752`; protocol mismatch → event + `disconnect()` + true `:754-759`; validators **insert** (union, never cleared) `:761-764`; `LoggedIn_Ready` + `ELoginSuccess` `:766-768`; `sendReady` `:770`) | `:532-775` (`_errorOss.str("")` `:544`) | `receive_data_from_server` `client.rs:1007-1168` (arms at `:1022,1025,1027,1033,1045,1050,1056,1066,1071,1076,1081,1086,1091,1102,1114,1132`) | PRESENT |
| `update()` (no socket → return `:779-782`; heartbeat then one receive `:784-791`; `catch(...)` → log `:786-793`) | `:777-794` | `update` `client.rs:1170-1183` (try/catch dropped — SEAM S11) | PRESENT |
| `saltAndHashPwd` (`SecretPasswordSalt.hpp` via `__has_include` `:801-803`, else `"salt"` `:805`; `sodiumHash(salt+password)` `:809-810`) | `:799-811` | `salt_and_hash_pwd` + `PASSWORD_SALT` `client.rs:604-618` (header absent from tree — verified) | PRESENT |
| `tryRegister` (state `Connected` gate; `name.empty()\|\|name.size()>32\|\|password.empty()` → `ERegistrationFailure{"Name or password fields too long or empty"}`; send) | `:813-830` | `client.rs:1186-1207` | PRESENT |
| `tryLogin` (same, `ELoginFailure`) | `:832-848` | `client.rs:1209-1227` | PRESENT |
| `tryLogoutFromServer` ((LoggedIn\|Ready) gate; state → `Connected` `:857`; token+name reset `:858-859` **before** the packet; `sendLogout`) | `:850-863` | `client.rs:1229-1238` | PRESENT |
| `tryDeleteAccount` / `tryRequestTopScores` / `trySendCompressedReplay` (unsupported level → **`true` no-op** `:894-899`) / `tryRequestOwnScore` / `tryRequestTopScoresAndOwnScore` / `trySendStartedGame` | `:865-939` | `client.rs:1240-1324` | PRESENT |
| `getState` / `hasRTKeys` / `getLoginName` / `addEvent` / `connectedAndInState` / `connectedAndInAnyState` / `pollEvent` (pop-front) / `isLevelSupportedByServer` | `:942-988` | `client.rs:1326-1353` | PRESENT |

**Events:** the 15-variant `std::variant` (`HexagonClient.hpp:52-77`) = Rust `Event`
(`client.rs:353-430`), variant order preserved, `E`-prefix dropped. All 15 variants are
emitted by exactly the C++ arms that emit them (diffed arm-by-arm above).

**State transitions:** all 7 states and every transition above are replicated, including the
quirks: `connect()` leaves `state = Connecting` when the socket is already connected
(`:397-401` ↔ `client.rs:927-931`); `tryLogoutFromServer` drops to `Connected` and clears the
session **before** sending the logout packet; `STCPServerStatus` sets `LoggedIn_Ready` +
`ELoginSuccess` **before** the `sendReady`/assert step.

**Globals/statics:** none (the class is fully self-contained; `sodiumHash`/`generateSodiumPSKeys`
/`calculateClientSessionSodiumRTKeys` are free functions in `Online/Sodium.cpp:18-83` →
`sodium.rs:122-183,196-205`).

**Side effects:** network I/O (socket), event queue, state, PS/RT keys, validator set. No
files, sounds, screenshots, or window ops in this module. All present in Rust.

### LeaderboardCache (C++ `Core/LeaderboardCache.cpp`, 76 lines)

| C++ (file:line) | Rust | Status |
|---|---|---|
| `receivedScores` (replace vector, stamp `_cacheTime`) `:18-24` | `received_scores` `leaderboard_cache.rs:95-104` | PRESENT |
| `receivedOwnScore` `:26-32` | `received_own_score` `:107-116` | PRESENT |
| `requestedScores` (create entry if absent, stamp time) `:34-38` | `requested_scores` `:119-128` | PRESENT |
| `shouldRequestScores` (no entry → true; age `> 6 s` strict) `:39-51` | `should_request_scores` `:131-138` (boundary pinned by test `:251-266`) | PRESENT |
| `getScores` (assert `hasInformation` + `map::at` fault when absent) `:53-57` | `get_scores` → `Option` `:141-150` | PRESENT (SEAM S7) |
| `getOwnScore` (pointer or `nullptr`) `:59-67` | `get_own_score` → `Option<Option<...>>` `:153-163` | PRESENT (SEAM S7) |
| `hasInformation` `:69-73` | `has_information` `:166-169` | PRESENT |
| `getSupported` — declared `LeaderboardCache.hpp:50`, **defined nowhere** | omitted | SEAM S8 (grep-verified dead declaration) |

"Invalidation" in this codebase = the 6-second TTL + whole-vector replacement on receipt
(there is no explicit invalidate call); the event-driven wiring (`EReceivedTopScores →
receivedScores` etc.) lives in `MenuGame.cpp:2616-2620` (out-of-scope pair, ported in
`oh-menu`).

### Database (C++ `Online/Database.cpp`, 391 lines)

All 17 public functions + the file-local `isLoginTokenTimestampValid` are present
(`database.rs:106-521`, verified by grep: 20 `pub fn`s incl. `open`/`schema_dump`):
`add_user :148`, `remove_user :159`, `dump_users :167`, `any_user_with_steam_id :173`,
`any_user_with_name :179`, `get_user_with_steam_id_and_name :187`, `get_all_users_with_steam_id
:207`, `get_user_with_steam_id :215`, `remove_all_login_tokens_for_user :226`, `add_login_token
:242`, `get_all_stale_login_tokens :257`, `remove_all_stale_login_tokens :268`,
`is_login_token_valid :282` (3600 s, `TOKEN_VALIDITY_SECONDS :67`), `get_top_scores :323`,
`add_score :356` (insert / no-op-when-`existing.value >= value` / in-place update reusing the
id — all three paths), `get_score :411` (join, value-desc, scan for the steam id, position =
row index), `execute :483` (`Batch` multi-statement + row printing). Schema DDL is pinned
byte-exact (`TABLE_*_DDL :71-73`, reverse `make_table` order, trailing-space quirk;
`tests/database.rs` asserts it against `tools/oracle/db_probe.cpp`). `SCClock`
(`Utils/Timestamp.hpp:13` = `system_clock`) ↔ `SystemTime`, incl. the signed-durational
future-timestamp-valid corner (`is_login_token_timestamp_valid :523-530`). The u64→i64
bit-cast binding (`bind_u64 :553`, `read_u64 :557`) matches sqlite_orm's
`sqlite3_bind_int64`/`sqlite3_column_int64` round trip. Records: `db_records.rs` 1:1 with
`DatabaseRecords.hpp:11-45`.

---

## MISSING

C++ behavior with no Rust counterpart (verified by grep; documented omissions cross-referenced
to the SEAMS entry that declares them).

| # | item | C++ evidence (file:line) | absence proof (command + result) | risk |
|---|---|---|---|---|
| M1 | **All diagnostic log output is absent.** The C++ client emits ~67 log call sites (`SSVOH_CLOG`/`_ERROR`/`_VERBOSE` + `fail`'s error stream): connection progress (`HexagonClient.cpp:105,109,399`), the ctor logging server ip/port **and both PS keys in plaintext** (`:455-463`), every received-packet arm (`:535,563,574,588,598,601,621,629,639,643,670,678,686,694,703,715,726,745`), `disconnect`/`update` exception logs (`:484,499,786-792`), and the 11 `_verbose`-gated "Sending …" lines (`:261,274,286,295,308,321,335,348,362,376,388` behind `if(_verbose)` `:34`, `_verbose{true}` `:448`). The C++ `Database` emits 14 `SSVOH_DLOG` sites (infra `Database.cpp:29-45`; `addUser :86-88`, `removeUser :95-96`, `dumpUsers :100-110`, `addLoginToken :164-166`, `addScore :304-306,319-321`, the two "Database integrity error" lines `:141-148,191-196`). Rust `oh-net` performs **no logging at all** (its only stdout output is `execute()`'s `print!("{},\t")` at `database.rs:501`, which mirrors the C++ `printf` callback `Database.cpp:362-377`). This is the operator-relevant "C++ does it, Rust doesn't" behavior — documented as an intentional deviation (SEAM S1), so also listed there. | `grep -rn "log::\|log!\|eprintln!\|warn!\|error!\|info!\|debug!" crates/oh-net/src/` → **no output (exit 1)**; `grep -c "SSVOH_CLOG\|clog" src/SSVOpenHexagon/Core/HexagonClient.cpp` → **67**; `grep -c "SSVOH_DLOG" src/SSVOpenHexagon/Online/Database.cpp` → **14**; `crates/oh-net/Cargo.toml` has no log dependency | internal |

No other C++ behavior (state transition, event, wire packet, gate, timer, query, or side
effect) was found without a Rust counterpart. Every other candidate was either PRESENT or a
documented equivalent — see SEAMS and DIFFERENT.

---

## SEAMS

Accepted, documented deviations (marker quoted). None of these hides missing *protocol*
behavior; each was checked against the C++ source, not just the comment.

| # | deviation | marker | notes |
|---|---|---|---|
| S1 | Logging dropped (M1) | `client.rs:59-64` ("**Logging**: the C++ `clog`/`SSVOH_CLOG*` calls … and the `_verbose` member are dropped — they are diagnostic only … and `oh-net` has no log dependency"); `mapping/networking.md` §10.3 item 1; `database.rs` module docs (data returned instead of logged) | the one MISSING item; risk internal |
| S2 | `HRClock::now()` / `std::this_thread::sleep_for` → injectable `ClientClock` (default `Instant::now` + `thread::sleep`) | `client.rs:65-68` ("**Time**: … go through [`ClientClock`] … so the 45 s heartbeat window and the 16-try ticket poll are testable"); `:414-455` | default behavior identical |
| S3 | `Steam::steam_manager&` → `Box<dyn SteamManager>` (the 4 methods the ctor polls) | `client.rs:69-70` ("**Steam**: … the [`SteamManager`] trait (the four methods the constructor polls, `Steam.hpp:41-66`)"); trait `:457-470` | verified against `include/SSVOpenHexagon/Core/Steam.hpp` |
| S4 | SFML `sf::TcpSocket` split into `NetSocket` + `FramedSocket` with injectable socket (`with_socket`) | `client.rs:71-101` (module docs "Socket layer"); `:637-670` | fork semantics verified against `/tmp/sfml_fork/src/SFML/Network/TcpSocket.cpp`: `receive(Packet&)` never returns `Partial` (raw layer returns Done/NotReady/Error/Disconnected only, `TcpSocket.cpp:272-305`); pending-frame accumulation + `disconnect()` reset mirrored (`:368-424, 213-219`) |
| S5 | `PASSWORD_SALT = "salt"` pinned (the secret-salt header is absent from the tree, so the C++ build *also* hashes `"salt"+password`) | `client.rs:71-79` ("**Password salt**: … the header is absent from the source tree (grep-verified) … the port pins that fallback"); `const PASSWORD_SALT :618` | `find . -name "SecretPasswordSalt*"` → no file; C++ `__has_include` at `HexagonClient.cpp:801-803`, fallback `:805` |
| S6 | `make_client_to_server_encrypted_packet` infallible vs C++ `bool` | `client.rs:775-792` comment ("the port is infallible (documented in `shared.rs`)"); `sodium.rs:218-221` ("`crypto_secretbox_easy` fails only for `mlen > SIZETYPE_MAX` (4 GiB)") | C++ failure path is `makeEncryptedPacketImpl`'s `crypto_secretbox_easy != 0` (`Shared.cpp:708-714`) — unreachable at packet sizes |
| S7 | `get_scores`/`get_own_score` return `Option` instead of C++ assert + `unordered_map::at` fault | `leaderboard_cache.rs:141-150,153-163` doc comments ("the port returns `None` for the absent entry … the callers … always check `has_information` first, `MenuGame.cpp:5608`") | C++ `LeaderboardCache.cpp:53-67` |
| S8 | `LeaderboardCache::getSupported` omitted — dead declaration (declared, never defined, never called) | `leaderboard_cache.rs:25-28` ("defined nowhere (grep-verified dead declaration, never called, never linked)") | `grep -rn "getSupported" src include` → only `include/SSVOpenHexagon/Core/LeaderboardCache.hpp:50` |
| S9 | DB: process-wide singleton + hardcoded `"ohdb.sqlite"` → `Database::open(path)`, per-value connection | `database.rs:1-50` module docs (D-004); `DECISIONS.md:45-57` ("**D-004 (SQLite entry)** — `rusqlite` (bundled)") | C++ `Database.cpp:40-79`; same DDL, same file format (`tests/database.rs` schema pin) |
| S10 | `Database::execute` cell rendering "not byte-pinned" (rusqlite native rendering vs `printf("%s,\t")` + glibc `(null)`) | `database.rs:459-481` doc ("a rendering equivalent … not byte-pinned (a server-console diagnostic, never wire data)") | C++ `Database.cpp:360-389`; multi-statement semantics matched via `Batch` |
| S11 | `update()`'s C++ `try/catch` dropped (Rust methods return `bool`, cannot throw) | `client.rs:1170-1173` comment ("the C++ `try/catch` around `ssvu` exceptions has no Rust analogue") | C++ `HexagonClient.cpp:783-793`; see DIFFERENT D2 for the one corner where this is observable |
| S12 | `getLoginName` returns `Option<&String>` vs `const std::optional<std::string>&` | `client.rs:80-83` doc; `:1338-1340` | C++ `HexagonClient.hpp:187-188`, `cpp:952-956` |
| S13 | `sodiumHash` → `[u8;32]`; the raw 32-byte digest wrapped as the wire `String` via `from_utf8_unchecked` | `client.rs:84-95` doc ("byte-exact … the string is never re-interpreted as text"); `:604-615` | C++ `Sodium.cpp:62-74` + `HexagonClient.cpp:809-810`; pinned by test `salt_and_hash_pwd_prepends_the_salt` `client.rs:2405` |
| S14 | Test doubles (`FakeSocket`, `SharedFake`, `ProgressSocket`, `FakeSteam`, `CtrlClock`) | `#[cfg(test)]` `client.rs:1368+`, `leaderboard_cache.rs:207+` | test-only, no production surface |
| S15 | `Database::schema_dump` — Rust addition, explicitly "No C++ equivalent" | `database.rs:449-458` ("No C++ equivalent (the C++ side inspects the file with the sqlite3 CLI)") | verification surface for the schema pin |
| S16 | `packet.rs` re-exports `oh_replay::Packet` instead of a fresh codec | `packet.rs:1-45` ("Contract note (D-014) … Re-exporting the one verified type instead of duplicating the codec") | byte-exact pins at `packet.rs:47-177` |

---

## DIFFERENT

Present but renamed or behaviorally different (highest impact first).

| # | item | C++ (file:line) | Rust (file:line) | delta + risk |
|---|---|---|---|---|
| D1 | **`expect()` panic vs C++ release-build assert + exception caught by `update()`** on a malformed server sequence. The C++ `SSVOH_ASSERT` is a **no-op in release** (`Global/Assert.hpp:20-40`); `std::optional::value()` on an empty optional then throws `bad_optional_access`. In the `STCPServerStatus` arm the state/event are set *first* (`:766-768`), so if a server sends `STCPServerStatus` without a prior `STCPLoginSuccess` (after key exchange, so it decrypts), the C++ throw is caught by `update()`'s `catch(...)` (`:786-793`) → logged, client survives (in `LoggedIn_Ready`, token-less). The Rust arm sets the same state/event then **panics** at `expect("STCPLoginSuccess set the token before the status")` → process unwinds/aborts. (The other `expect()` sites — ticket at `client.rs:1202,1220,1236,1245` and token at `:1258,1279,1293,1306,1319` — are guarded by state gates that make them unreachable in *both* implementations.) | `HexagonClient.cpp:770` (`sendReady(_loginToken.value())`) + `:766-768` + `:777-794` (catch); `Assert.hpp:20-40` | `client.rs:1132-1167` (arm), `:1158-1163` (expect) | reachable only with a hostile/malformed server; C++ degrades, Rust crashes. risk: **internal** (upgrade to uservisible-gameplay only under attack) |
| D2 | **`_lastHeartbeatTime` initial value**: C++ default-constructs to the epoch (`HexagonClient.hpp:114` `HRTimePoint _lastHeartbeatTime;`, `cpp:447` `_lastHeartbeatTime{}`) vs Rust `Instant::now()` at construction. The Rust doc claims this is "unobservable: the first heartbeat is sent in the constructor's `connect()`" — true on the success path, but not when the **first heartbeat send fails inside `connect()`** (e.g. non-blocking send would-block): neither side clears `_socketConnected` (`:425-428` / `client.rs:938-943`), so `update()` proceeds; C++ then sees `now - epoch > 45 s` and **retries the heartbeat on the next frame** (`:514-516`), while Rust waits up to 45 s after construction before even attempting it (`client.rs:994-1002`). Wire-traffic-identical; only the socket-teardown timing differs, in a state where a 7-byte send on a fresh connection would-block (near-impossible in practice). | `HexagonClient.cpp:447,508-530`; `HexagonClient.hpp:114` | `client.rs:575-580` (field + "unobservable" claim), `:652` (init), `:991-1004` | risk: **internal** |
| D3 | **Stale C++ line citations in `client.rs` comments** (D-014, `DECISIONS.md:287-298`, is exactly about this). Behavior verified present on both sides; only the cited lines drifted (by 1–26 lines, systematic in `receiveDataFromServer` and its neighbors): PInvalid `:547-551`→actual `:552-556`; PEncryptedMsg `:553-556`→`:558-561`; STCPKick `:558-565`→`:563-570`; STCPPublicKey `:567-604`→`:573-617`; STCPLoginSuccess `:621-639`→`:636-657` (incl. the claim "the login handler … `HexagonClient.cpp:638`" → actual `:656`); the `:728-731`/`:733-738` mismatch checks → actual `:749-752`/`:754-759`; validators "INSERTed … `:740-743`" → actual `:761-764`; STCPServerStatus `:717-772` → `:743-773`; `_errorOss.str("")` `:541` → `:544`; failEvent `:403-412` → `:404-412`; ctor InitError `:462-466` → `:464-470`; `initializeTcpSocket` `:104`/`:107`/`:114` → `:98`/`:102-104`/`:111`; `disconnect` `setBlocking` `:485` → `:486`. | verified against `src/SSVOpenHexagon/Core/HexagonClient.cpp` (grep/sed above) | `client.rs:702-719, 925-1006, 1007-1167` comment blocks | citations only; no behavior delta. risk: **none** |
| D4 | **`(D-014)` marker misattribution**: `client.rs:9` "## Contract correction (D-014)" and `client.rs:1568` "the D-014 contract correction" attach the pull-model contract correction to D-014, but `DECISIONS.md:287` D-014 is "Verify a worker's FILE CITATION, not just its claim" (a process rule). The correction itself is recorded in the `client.rs` module docs (`:9-22`) and `mapping/networking.md` §10.0, correcting `17-net-client-db.md:14` ("background thread + queues" — false; grep-verified no `std::thread` in the C++ client, proof P1). There is **no D entry** for the contract correction. | `DECISIONS.md:287-298`; `17-net-client-db.md:14` | `client.rs:9, 1568` | marker/bookkeeping only; the underlying correction is real and verified. risk: **none** |
| D5 | **Send-resume offset location**: C++ keeps `m_sendPos` *inside the `sf::Packet`* (fork `Packet.hpp:435`, updated in `TcpSocket.cpp:356-363`; **not** reset by `Packet::clear()`, fork `Packet.cpp:82-87`), so a stale offset from a failed partial send survives into the next send (incl. the unsigned-underflow corner, `TcpSocket.cpp:357-358`). Rust moves the offset to `FramedSocket` (`send_pos`) and reproduces the underflow as `SendResult::Error`. For this client (single `_packetBuffer` member) the observable behavior matches; pinned by `framed_send_resumes_from_partial_progress` (`client.rs:2281-2304`). | fork `Packet.cpp:82-87`, `Packet.hpp:435`, `TcpSocket.cpp:309-365` | `client.rs:270-281, 318-360` | equivalent; structural refactor. risk: **none** |
| D6 | Renames (mechanical): `LoggedIn_Ready`→`LoggedInReady`; `E*` structs → `Event::*` variants; `pollEvent`→`poll_event`; camelCase→snake_case; `sendPacket(sf::Packet&)`→`send_packet()` using the member buffer; `update()`'s member-borrow handled by `mem::replace` (`client.rs:1174-1183`, semantics identical — the C++ packet is cleared before every send/reuse, fork `Packet.cpp:82-87` + `Shared.cpp:725-728`). | `HexagonClient.hpp:41-77`, `cpp` throughout | `client.rs:325-430, 1170-1183` | risk: **none** |

---

## VERDICT

**PASS with documented deviations — this pair is behaviorally 1:1**: every C++ state
transition, event, gate, timer (16-try ticket poll, 45 s heartbeat, 6 s cache TTL, 3600 s
token validity), wire packet, kick path (`EKicked` + disconnect), and all 17 DB queries have
a verified Rust counterpart; the only true missing behavior is the dropped diagnostic
logging (M1, documented, risk: internal), with two adversarial/edge-case divergences (D1
panic-vs-exception on a malformed server status; D2 heartbeat-timer init corner, both
internal) and a batch of stale C++ line citations (D3) to clean up.

*Most important gap if the operator demands literal 1:1:* **M1 — the C++'s ~81 log call
sites (client + database) have no Rust counterpart** (documented as an accepted deviation;
restore behind a `log`-crate sink if log parity is required).
