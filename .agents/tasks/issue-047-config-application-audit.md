# Task 047 — do the configs actually APPLY? (the per-platform application audit; Android + wasm in focus)

Read `.agents/WORKER-HYGIENE.md` first. Operator directive, verbatim: *"yeah queue. generally check
for configs that don't apply on android and wasm builds"* — i.e. fix the fullscreen gap below, and then
answer the general question with evidence rather than impressions.

## The finding that prompted it (established, not speculative — verify it, then generalise)

`fullscreen` exists in the C++ config (`Config.cpp:320-324`: `fullscreen`,
`fullscreen_auto_resolution`, `fullscreen_width`, `fullscreen_height`; defaults false / false / 1920 /
1080) and in the port's `oh-config` (`crates/oh-config/src/config.rs:327-331`). But a grep over
`crates/` finds the field **only in oh-config** — nothing READS it. The C++ *applies* it at window
creation (`extlibs/SSVStart/include/SSVStart/GameSystem/GameWindow.hpp:90`:
`fullscreen ? sf::Style::Fullscreen : sf::Style::Default`, via `Config::setFullscreen(GameWindow&,
bool)` at `Global/Config.hpp:38`, with `applyAutoFullscreenResolution` at `Config.cpp:485`). So a
`fullscreen = true` config file currently does nothing in the port. Fix that on desktop, and then ask
the general question for every other setting.

## The mission

For EVERY config setting the C++ has, answer three questions, with the citation for each:

1. **Where does the C++ APPLY it?** (window creation / the renderer / audio / a per-frame read)
2. **Where does the PORT apply it, per target** — desktop, Android, wasm?
3. **Verdict per target:** APPLIED · INERT-BY-DESIGN (with the reason — e.g. a window style that has no
   phone or browser equivalent, and where the C++'s own platform path is equally inert) ·
   **MISSING** (the port is inert where the C++ is not — these are the bugs this task exists to find).

The MISSING rows are the deliverable. The INERT-BY-DESIGN rows need one line of justification each —
that line is what separates an audit from a list. APPLIED rows are just the table.

## Method (borrow the shapes that already worked, do not invent)

- **The denominator is the C++'s config table** (`Config.cpp`'s `X(name, type, json_key, default)`
  rows), not the port's field list — the port must mirror the C++ (the parity rules in the plan's
  mapping docs). Enumerate it completely; a sampled audit is not an audit.
- Cross-check the port's `oh-config` field list against it: a C++ row with no port field is a separate
  finding (report it, do not silently add it).
- Detection for "does the port apply it": **grep the port for READS of the field** (including indirect
  reads through a getter). A field that is only declared, loaded and synced is inert — that grep is the
  whole method, and it is how the fullscreen gap was found.
- For each APPLIED config, check the target-specific paths too: the emscripten window, the Android
  surface (see 25j's swap-interval work, which is this class done right), the desktop window.
- Where the C++ applies something through a helper, cite the helper (e.g. `setFullscreen`,
  `applyAutoFullscreenResolution`, the volume setters) rather than the call site alone.

## What fixing means (keep it proportional, and do not invent behaviour)

- Fix the MISSING rows that are small and platform-appropriate. `fullscreen` +
  `fullscreen_auto_resolution` on **desktop** is the first: mirror `GameWindow.hpp:90`'s creation-time
  application (and the auto-resolution helper where it genuinely applies). On **Android and wasm** both
  are INERT-BY-DESIGN — a fullscreen *window style* has no equivalent there, and the web's own
  fullscreen affordance is lane 046's browser-API button (cross-reference it, do not duplicate it).
- **§3 discipline: if the C++ is inert on a platform, inert is PARITY.** Say so and move on — do not
  "fix" a platform where the original does nothing, and do not invent a setting's meaning.
- Anything that turns out to be a large change: file it as a finding with a size estimate and report;
  do not rabbit-hole inside one setting.

## Acceptance

- A table in the report covering **every** config row: the setting · the C++ application site · the
  port's site · desktop · android · wasm · verdict (with the INERT-BY-DESIGN reason, or the MISSING
  note). The whole denominator, no "I sampled".
- The fixes committed, each carrying its C++ citation; the INERT rows justified; the remaining MISSING
  rows filed with sizes.
- Gates: the host gate before each commit, `cargo check --target aarch64-linux-android` (§4 — you are
  touching window-creation paths that all targets share), and the wasm build in the emsdk container if
  you touch a web path.
- NOTE: lane 046 is concurrently editing `crates/oh-web/` and the web shell. Keep your web-side edits
  minimal and expect a small merge at landing; do not edit `crates/oh-web/site/index.html` at all.
