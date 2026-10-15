# Issue 19 — `oh-server-control` + `oh-workshop-uploader` tools

## Objective

Port the two auxiliary binaries: `src/OHServerControl/main.cpp` (talks to the
server control port) and `src/OHWorkshopUploader/main.cpp` (739 lines; builds and
uploads a workshop item from a pack).

## Key decisions

- Mirror each `main.cpp` 1:1, including exact CLI arguments and output text.
- `OHWorkshopUploader` uses the Steam UGC API → depends on issue 20's Steam
  bindings; if that is not ready, define the seam and stub, and defer the wiring.
- Both reuse `oh-net` (control protocol) and `oh-data`/`oh-assets` (pack reading).

## Files

| File | Change |
|---|---|
| `crates/oh-server-control/Cargo.toml`, `src/main.rs` | new |
| `crates/oh-workshop-uploader/Cargo.toml`, `src/main.rs` | new |

## Steps

1. Port `OHServerControl` argument parsing + control-port commands.
2. Port `OHWorkshopUploader` argument parsing, pack packaging, metadata, upload.

## Verification

- `--help` output matches the C++ (capture the C++ help text and diff).
- For the uploader, a dry-run mode produces the same staging directory/zip layout
  as the C++ (if the C++ has one; otherwise test the packaging function directly).
- `tools/gate.sh` green (both binaries build on Linux + Windows).

## Notes / risks

- Uploader likely depends on Steam runtime presence; make the upload itself
  testable behind a trait so CI does not need Steam.
