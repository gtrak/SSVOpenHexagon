# Issue 18 — `oh-net` server (HexagonServer)

## Objective

Port `HexagonServer` (1430 lines): the authoritative game server — socket accept
loop, per-client sessions, the encrypted handshake, login/registration,
score submission with validation, top-score queries, replay storage/validation,
server status, and the `-server` mode of the main binary.

## Key decisions

- Mirror `Core/HexagonServer.hpp` + `.cpp` 1:1.
- Reuse issue 16's protocol + issue 17's database.
- Threading model copied from the C++ (accept thread, per-client handling).
- Score validation uses `Utils::LevelValidator` + the server's level whitelist.

## Files

| File | Change |
|---|---|
| `crates/oh-net/src/server.rs` | new |
| `crates/oh-net/tests/server_flow.rs` | new — end-to-end register/login/submit |

## Steps

1. Port the server struct, socket/listener setup, and accept loop.
2. Port per-client session state and the packet handling for every `CTSP*` type.
3. Port login/registration/token issuance (with the same hashing as the C++).
4. Port score submission + validation + DB write, top-scores queries.
5. Port replay upload/validation and the server status response.
6. Wire `-server` mode to start the server.

## Verification

- End-to-end loopback: Rust client (issue 17) registers, logs in, submits a score,
  queries top scores, uploads a replay — assert the DB state.
- A test that a replay rejected by `LevelValidator`/whitelist produces the same
  rejection the C++ does.
- Optional: if reachable, connect the Rust client to the real C++ server and the
  real client to the Rust server; record the result (blocked if no network).
- `tools/gate.sh` green.

## Notes / risks

- Password hashing: use the exact scheme from the C++ (likely sodium
  `crypto_pwhash` or `crypto_generichash` — check `Shared.cpp`/`HexagonServer.cpp`)
  or accounts created by the C++ server will not authenticate.
