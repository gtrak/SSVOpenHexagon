# Issue 22 — Windows + Linux builds, CI, cross-compile check

## Objective

Make the whole workspace build and test on Linux and Windows, with CI proving
both, and a cross-compile check for the other target where practical.

## Key decisions

- Linux: `x86_64-unknown-linux-gnu` native.
- Windows: support both `x86_64-pc-windows-gnu` (MinGW, cross-compilable here) and
  `x86_64-pc-windows-msvc` (native/CI). The C++ build supported MinGW and MSVC, so
  both should work; if a dependency is MSVC-only or GNU-only, document it.
- CI: GitHub Actions running the gate on `ubuntu-latest` and `windows-latest`.
- Cross-compile: `cargo check --target x86_64-pc-windows-gnu` on Linux as a fast
  signal (full link may need MinGW + the native libs).

## Files

| File | Change |
|---|---|
| `.github/workflows/rust.yml` | new — CI matrix |
| `tools/build-windows.sh` | new — cross-build helper |
| `.cargo/config.toml` | new — target linker if needed |

## Steps

1. Add the Windows targets and get `cargo build --workspace` green for each
   (resolving any unix-only APIs found in earlier issues).
2. Add the CI workflow running `tools/gate.sh` on both OSes.
3. Add the cross-compile check script.
4. Ensure the native libs (Steam/Discord) are handled per-OS (either bundled from
   `_RELEASE` or behind features).

## Verification

- CI green on both OSes.
- `cargo check --workspace --target x86_64-pc-windows-gnu` succeeds locally if the
  target + MinGW are present; otherwise the CI job is the proof.
- `tools/gate.sh` green on Linux.

## Notes / risks

- Steamworks/Discord SDK native libraries are platform-specific; keep them behind
  features so the workspace builds without them.
