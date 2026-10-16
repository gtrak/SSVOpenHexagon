# Issue 019 — final worker report (deliverable chain complete)

## Deliverable commits (branch `pi-subagents/worker-Implement-issue-019-in-home-gary-dev-SSVOpenHexagon-work-in-your-managed-git-wor-07135f09-e380b73-b067-s0-t0`)

| # | Commit | Content |
|---|---|---|
| a | `dc3b8a5c` | `crates/oh-server-control` (1:1 port of `src/OHServerControl/main.cpp`) + 11 tests (6 in-crate + 5 loopback: CLI parsing, exact datagram bytes against an in-process listener, exact output text) |
| b | `73cf921c` | `crates/oh-workshop-uploader` (1:1 port of the interactive menu + iostream primitives + `SteamUgc` seam) + 36 tests (11 in-crate + 25 integration with a scripted seam double, byte-exact output pinning) |
| c | `7635acd8` | gate run + `.agents/plans/STATUS.md` row 19 → LANDED + `.agents/plans/001-rust-port/19-tools.md` D-014 corrections |

Final commit: **`7635acd8`**. Worktree base: master `67a28d3b`.

## What the server-control protocol turned out to be (D-014 verified)

- No CLI arguments (the contract's "argument parsing" claim did not apply).
- Each stdin line → **ONE UDP datagram to `127.0.0.1:50506`** (control port, `Global/Config.cpp:352` default; `main.cpp:34-41`).
- Datagram = an `sf::Packet` holding a single string: **u32-BE byte count + raw bytes** (SFML fork `sf/net/Packet.cpp:515-521`).
- Server side: `HexagonServer.cpp:316-368` (`packet >> controlMsg`, split on whitespace; `verbose` implemented, `db exec` is `#if 0`).
- **Not** the `'oh'`-preamble game protocol in `oh_net::shared`. The Rust port reuses `oh_net::packet::Packet` (re-export of `oh_replay::Packet`, whose `write_str` is the identical u32-BE string codec) for encoding only; sending is std `UdpSocket`. No `oh-net` file was modified (extend-only honored).

## Steam seam shape

- `SteamUgc` trait (raw SDK surface, no logging): `init()->bool`, `shutdown()`, `create_item()`, `start_item_update(u64)->Option<UpdateHandle>`, `set_content(handle, &Path)->bool`, `set_preview(handle, &Path)->bool`, `submit_update(handle, note)`, `run_callbacks()`, `take_create_item_result()->Option<CreateItemOutcome>`, `take_submit_item_result()->Option<SubmitItemOutcome>`; `UpdateHandle=u64`, `SteamResult(i32)` + pinned `result_to_string` EResult table, `CreateItemOutcome{Ok(u64),IoFailure,Error(SteamResult)}`, `SubmitItemOutcome{Ok,IoFailure,Error(SteamResult)}`.
- `SteamHelper<'a,S>`: owns the `[Steam]` log lines, the pending-operation counter, and the callback-handler order (result line → CLI continuation → "Removed pending operation"); drains results after `run_callbacks`.
- `NoSteam` (default, no SDK): `init` → false → `[Main] Could not initialize Steam API, exiting...` + exit 1 (byte-exact vs C++ without Steam); all other methods `unimplemented!` with `TODO(issue-20)` markers. Issue 20 replaces `NoSteam` with the real Steamworks binding.
- `lib.rs::run(steam_sdk, input, out) -> i32`; binary `main` wires real stdin/stdout (the C++'s `main.cpp:711-737` shape).

## Contract corrections (D-014, applied in commit c)

`src/OHWorkshopUploader/main.cpp`'s `main` takes **NO arguments** — it is a purely interactive stdin/stdout menu (create new workshop item / upload contents of an existing item / set preview image / exit). The original contract's claims of "argument parsing, pack packaging, metadata, upload", "`--help` output matches the C++", "dry-run mode", and "reuse `oh-data`/`oh-assets` (pack reading)" were all wrong and are corrected in `19-tools.md` (Objective, "What the C++ actually does", Key decisions, Steps, Verification rewritten to what was actually run).

## Test counts

- Issue 19: 47 tests (11 server-control + 36 uploader), all green.
- `cargo test --workspace`: 600 passed / 0 failed (67 test binaries).
- `cargo test --workspace --features oh-game/produce-lua-metadata`: 600 passed / 0 failed.
- `bash tools/gate.sh`: GATE_EXIT=0 for all 4 stages (fmt / build / clippy `-D warnings` / test).
- Both binaries build on Linux (`target/debug/oh-server-control`, `target/debug/oh-workshop-uploader`).

## Documented deviations (byte-pinned in tests)

1. EOF in the tool's retry loops: the C++ would spin forever re-prompting; the port prints the prompt once and exits 1. Shutdown lines still print on every exit path (the C++ destructor runs on all paths).
2. `read_string` (main.cpp:113-123) is dead code in the C++ (no call sites); ported for the 1:1 utility set and `#[allow(dead_code)]` with a comment.
3. The `Error code: ''` double-quote for a missing/wrong-kind path (empty `std::error_code`); real OS failures print the verbatim `strerror` (ELOOP case pinned).

## Hard exclusions honored

`git diff --stat 67a28d3b..HEAD -- crates/oh-net crates/oh-app` is empty; no simulation-order, D-019, or determinism-harness changes.

## Residual risks

- The real Steamworks wiring is issue 20's (`NoSteam` default ⇒ shipped binary exits 1 with the exact C++ no-Steam message — same behavior as the C++ on a machine without Steam).
- The `strerror` text is C-locale in both implementations (the C++ `ec.message()` honors the current locale; the tools run in the default C locale, and the port pins that).
- Windows build of the two new crates not verified in CI here (the gate is Linux; the code is platform-neutral std-only + no `std::os::unix` in non-test code — `std::os::unix` appears only under `#[cfg(unix)]` in the ELOOP test).

## Recommended next step

Run the review gate over the three commits (a/b/c), then merge the worktree branch to master (squash or rebase as the orchestrator prefers) so issue 20 can build on the `SteamUgc` seam.
