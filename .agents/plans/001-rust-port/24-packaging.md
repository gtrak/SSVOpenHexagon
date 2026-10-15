# Issue 24 — Packaging: `_RELEASE` layout, asset bundling

## Objective

Produce distributable builds that run against (or bundle) the existing `_RELEASE`
assets, matching the C++ layout so players can drop the Rust binary into an
existing install.

## Key decisions

- The C++ install puts the binary in `_RELEASE/` alongside `Assets/`, `Packs/`,
  `ConfigOverrides/`, `config.json`, and the Steam/Discord natives. The Rust build
  must run from that same directory with no layout changes.
- Packaging scripts build release artifacts for Windows and Linux.

## Files

| File | Change |
|---|---|
| `tools/package.sh` | new — builds + assembles a release dir per OS |
| `release/` docs | new — what ships |

## Steps

1. Add release profile settings (`[profile.release]`) mirroring the C++ release
   intent (optimization; the C++ had LTO commented out — note whether we enable it).
2. Write `package.sh` to assemble `_RELEASE`-compatible output for Linux and
   (cross) Windows.
3. Verify the packaged binary runs from a clean `_RELEASE` copy.

## Verification

- Package, then run `open-hexagon -headless -p base -l <level>` from the packaged
  directory; exit 0 and correct score.
- `tools/gate.sh` green.

## Notes / risks

- Do not modify the existing `_RELEASE` assets; the package step copies them.
- Keep the C++ build artifacts and the Rust ones clearly separated.
