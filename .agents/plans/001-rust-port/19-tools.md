# Issue 19 — `oh-server-control` + `oh-workshop-uploader` tools

## Objective

Port the two auxiliary binaries: `src/OHServerControl/main.cpp` (68 lines; talks
to the server control port) and `src/OHWorkshopUploader/main.cpp` (739 lines;
an interactive menu that creates/uploads Steam workshop items).

## What the C++ actually does (D-014 verified against source)

- **`OHServerControl`**: takes NO arguments. Reads lines from stdin; each line
  is sent as ONE UDP datagram to `127.0.0.1:50506` (the control port,
  `Global/Config.cpp:352` default) — main.cpp:34-41. The datagram is a
  `sf::Packet` holding a single string: `u32-BE byte count + raw bytes`
  (the SFML fork's `Packet::operator<<(const std::string&)`,
  `sf/net/Packet.cpp:515-521`). This is NOT the `'oh'`-preamble game protocol
  in `oh_net::shared`; the server side reads it back with `packet >>
  controlMsg` and splits on whitespace (`HexagonServer.cpp:316-368`;
  `verbose` is implemented, `db exec` is `#if 0`).
- **`OHWorkshopUploader`**: its `main` (main.cpp:711-737) takes NO arguments —
  it is a purely interactive stdin/stdout menu: create a new workshop item /
  upload contents of an existing item / set the preview image of an existing
  item / exit. There is NO `--help`, no pack reading, no zip/staging, no
  metadata building, and no dry-run mode. The Steam UGC calls
  (`ISteamUGC`-style create/start-update/set-content/set-preview/submit +
  callback polling) are the only platform dependency.

  (The earlier contract text claimed "argument parsing, pack packaging,
  metadata, upload" and "reuse `oh-data`/`oh-assets` (pack reading)" — both
  wrong; corrected here per D-014.)

## Key decisions

- Mirror each `main.cpp` 1:1, including exact output text. (There are no CLI
  arguments to mirror — both binaries are interactive.)
- `oh-server-control` reuses `oh-net` for the packet STRING encoding
  (`oh_net::packet::Packet` — u32-BE length + bytes, the same `sf::Packet`
  string codec); the socket send is std `UdpSocket` (one datagram per line,
  to port 50506). No `oh-net` file changes needed.
- `OHWorkshopUploader` uses the Steam UGC API → the real Steamworks wiring is
  issue 20. Issue 19 defines the seam: a `SteamUgc` trait (raw SDK surface) +
  `SteamHelper` (the `[Steam]` log lines, pending-operation counter, and
  callback-handler order: result line → CLI continuation → "Removed pending
  operation") + `NoSteam` (default no-SDK impl: `init` fails →
  `[Main] Could not initialize Steam API, exiting...` + exit 1; the other
  methods are `unimplemented!` with a `TODO(issue-20)` marker). No `oh-data` /
  `oh-assets` / `oh-net` dependency.
- iostream fidelity: `cin >> int` leaves the first non-digit in the stream
  (`num_get`) — implemented via a 1-byte pushback scanner; `std::getline`
  drops only `\n` (keeps `\r`); a failed `getline` discards a partial line;
  EOF in the tool's retry loops prompts once and exits 1 (the C++ would spin
  forever on the same prompt) — documented deviation.
- `std::filesystem` error text: a missing path or wrong-kind path leaves the
  `error_code` default → `Error code: ''` (two single quotes, empty); a real
  OS failure prints the verbatim `strerror` (e.g. ELOOP →
  `Too many levels of symbolic links`).

## Files

| File | Change |
|---|---|
| `crates/oh-server-control/Cargo.toml`, `src/lib.rs`, `src/main.rs`, `tests/loopback.rs` | new |
| `crates/oh-workshop-uploader/Cargo.toml`, `src/{lib,main,cli,input,readers,steam}.rs`, `tests/uploader.rs` | new |
| `Cargo.toml` (workspace members) | both crates added |

## Steps (as executed)

1. Port `OHServerControl`: stdin line → one 50506 UDP datagram (string
   codec), exact `[Main] Sent control port message` / read-error output.
2. Port `OHWorkshopUploader`: the four menu flows + readers + `SteamUgc`
   seam (`NoSteam` default); the Steam SDK wiring stays in issue 20.

## Verification

- `oh-server-control`: CLI parsing tests + a loopback test against an
  in-process listener asserting the EXACT command bytes (u32-BE length +
  string) and the exact output text; 11 tests.
- `oh-workshop-uploader`: the C++ cannot be compiled without the Steam SDK,
  so the output text is transcribed from `main.cpp` and pinned (allowed
  alternative to "capture and diff" per the task). 36 tests: the iostream
  primitives (overflow, prefixes, getline CR, token scan), and full
  interactive flows through a scripted `SteamUgc` double with byte-exact
  output pinning of every success/error branch (including the
  `k_EResult` name table and the empty-`Error code` behavior).
- `tools/gate.sh` green (both binaries build on Linux).

## Notes / risks

- The uploader's upload path is testable without Steam thanks to the
  `SteamUgc` seam; issue 20 replaces `NoSteam` with the real Steamworks
  binding (the seam's method shapes match `ISteamUGC`'s UGC call set).
- Both binaries build on Linux in CI; the C++ builds for Windows+Linux.
