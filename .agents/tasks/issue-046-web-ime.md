# Task 046 — the web build's IME: a phone must be able to type a profile name

Read `.agents/WORKER-HYGIENE.md` first. Operator report, verbatim: *"i hear blips on wasm on my
phone. but i need similar IME input to enter my name as the android port uses"* — i.e. the WASM build,
tested on a real phone browser, reaches the name-entry screen (`ETLPNewBoot`) and **there is no way to
type**. This is the web twin of 025f.

## Why raw keydown cannot work (the diagnosis, established by 025f's sibling)

The web's text path (issue 032) converts `keydown` events into the port's `TextEntered`. That is fine
on a desktop and **useless on a phone**: a mobile browser only raises the on-screen keyboard when a
real editable element is focused, and a keydown listener on the canvas never does that. So the
character pipeline is correct and untriggerable — exactly the Android situation 025f addressed.

## The precedent to MIRROR, not reinvent (025f, D-031)

- The C++ has NO web text path to transcribe either (its input came from SFML), so — like 025f — this
  is a **deliberate platform addition** and the acceptance is *human usability on the device*, not code
  shape. It needs the same kind of register row: add an `F32` entry to
  `.agents/plans/001-rust-port/deviations.md` in the F28/031/F31 class, stating what is added, why the
  C++ has no equivalent, and that the desktop path is unchanged.
- 025f's shape is the shape to copy: `crates/oh-app/src/android.rs` exposes
  `show_soft_input(true)` / `hide_soft_input(false)`, and they are called on the menu's
  **`is_entering_text` transition edge** — computed ONCE per transition (not per frame; see
  `windowed::MenuSlotAdapter::refresh_input`'s docs, which the Android module also cites). The
  per-frame call is the bug class the operator hit with touch input; do not repeat it.
- The character pipeline is shared and already works: the Window's entered-text sink →
  the menu's `onEvent(TextEntered)` → `entered_chars`, gated by the ported `isEnteringText()`
  (`MenuGame.cpp:2862-2878`). Your job is to FEED that sink from the IME, not to build a new one.

## What to build

1. A hidden-but-FOCUSABLE input element (the committed shell `crates/oh-web/site/index.html` is copied
   into the package by `tools/package-web.sh`; an element created from Rust is equally acceptable if
   you prefer — the web layer already gets-or-creates `<canvas id="c">`). It must NOT be
   `display: none` (browsers refuse to focus those): position it off-screen, `opacity: 0`, and set
   `inputmode="text"`, `autocapitalize="off"`, `autocomplete="off"`, `spellcheck="false"`,
   `enterkeyhint="done"`, and a **16px font size** (a smaller font makes iOS zoom the page when it
   focuses).
2. Focus it on entering a text-entry state and blur it on leaving, from the SAME `is_entering_text`
   edge site the Android path uses — once per transition.
3. Feed its text into the existing entered-text sink, and get these four cases right — they are the
   whole task:
   - **Printable text** via the input's `input` event (works for the soft keyboard AND for desktop
     typing into the focused element).
   - **Backspace**: a deletion arrives as an `input` event with
     `inputType: deleteContentBackward` (NOT as a keypress), and the port's TextEntered path already
     handles unicode 8 (`window.rs`'s `pop_back` arm, the C++'s second entry). Route it as that.
   - **Enter**: it does not produce an `input` event either, and in this flow Enter is what CREATES the
     profile. Match whatever the 025d/025f device flow proved works (the Android port's Enter created
     the profile) rather than inventing a new key meaning.
   - **Composition** (`compositionstart`/`update`/`end`): a phone keyboard, an autocomplete or any CJK
     method will use it. Do not let it corrupt or double the characters.
4. **DO NOT DOUBLE-TYPE.** This is the main hazard and the most likely regression: with the hidden
   input focused, the browser may still deliver `keydown` to the canvas, so the same character arrives
   through both paths. Route text through exactly ONE of them while the IME element is focused, and
   prove it in a test — the desktop typing path must produce exactly one character per key (the port
   already documents that it deliberately suppresses the paired KeyPressed for text events at
   `window.rs:1777-1790`; do not break that).
5. All text-entry states must benefit, driven by `is_entering_text()` rather than special-casing the
   name-entry screen: the port also has the dialog input box and the Lua console.

## Verification (what a lane can prove, and what only the operator can)

- `node tools/web-probe.mjs <url> 30 --keys=Enter` reaches `menu:ETLPNewBoot`; then prove the IME path
  PROGRAMMATICALLY with CDP's `Input.insertText` (a genuine IME-style commit, not a keystroke): type a
  name, confirm the game received it, and confirm the profile is created (the same observable 025d
  used on device).
- Confirm `document.activeElement` is your IME element exactly while the state is a text-entry state,
  and that it is blurred otherwise (a stray focused input would swallow the game's keys).
- Emulate a phone viewport (`Emulation.setDeviceMetricsOverride` + touch emulation) and re-run: the
  element must still focus. The soft keyboard itself cannot be raised headlessly — say so plainly.
- The desktop keyboard path must be unregressed (the double-typing test above), and the host gate must
  stay green.
- **The last word belongs to the operator's phone**: your report must state exactly what they should
  do and see, and what remains unproven without it. A precise "the IME raises on the phone, probe
  verified everything else" beats a claim.

## Boundaries

Web only: `crates/oh-web/`, the `windowed` edge hook, `crates/oh-render/src/window.rs` if the feed
genuinely needs it. Do NOT touch the Android path (025f works — the operator is comparing against it),
do not change the C++-faithful semantics for non-text states, build the wasm in the emsdk container
(§4: the host gate is not a web gate) plus the host gate before each commit, commit with explicit
paths, bound everything with `timeout`, and append a `## Report`.

## ADDED BY THE OPERATOR WHILE THIS LANE RUNS: a fullscreen overlay button ("kind of out of the way")

Context they gave: the IME target is **Android Chrome**, and they also want a **fullscreen toggle
button on the web build, placed out of the way**. Same lane, same files (`crates/oh-web/site/
index.html` + the web layer), so it rides with the IME rather than starting a second lane that would
collide on those files.

**Parity status, checked before writing this:** the C++ HAS fullscreen, but only as *config* —
`fullscreen`, `fullscreen_auto_resolution`, `fullscreen_width/height` (Config.cpp:320-324, defaults
false/1920/1080) applied once at window creation via `sf::Style::Fullscreen` (GameWindow.hpp:90), with
`Config::setFullscreen(GameWindow&, bool)` (Config.hpp:38) as the setter. There is **no runtime
fullscreen bind** in the bind table, so an emscripten C++ would show no browser fullscreen either.
This is therefore a **second deliberate platform addition** and needs its own register row — **F33**,
beside the IME's F32.

**The shape that keeps the config as the single source of truth:** the button toggles the ported
`fullscreen` config value (`oh-config`'s `fullscreen` LinkedValue, which the C++'s own settings menu
writes) and the WEB applies it through the browser Fullscreen API. Do not introduce a second state
that the config does not know about; if the port's settings menu can already flip that value, the
button and the menu must agree.

**Placement — "out of the way", and the port's own furniture says where:** the gameplay overlay's
buttons are bottom-middle (`BUTTON_HALF 32`, `PAD 12`, `MIDDLE_GAP 80` in `touch_overlay.rs`) and 039's
F31 action row is along the TOP (y≈44), while the online-status sprite is top-right. So a small
translucent button in a corner — bottom-right preferred — is the one place that collides with nothing.
Make it a real touch target (≈44 px), keep it subtle, and give it `pointer-events: auto` on itself
only, so it cannot swallow the game's drags outside its own box.

**Two things to get right rather than discover later:**
- `fullscreenchange` must update the button's state, because Esc (and Android's back gesture) can leave
  fullscreen without the game asking.
- Entering or leaving fullscreen RESIZES the page, so the canvas must come back at the right size —
  032's real-size path (`Resized` → the canvas backing store + GL viewport) should handle it; VERIFY
  that rather than assuming, since a stale canvas size in fullscreen is exactly the 025g class of bug.

**Honest limit to state, not paper over:** `requestFullscreen()` requires a transient user activation,
and the CDP probe's synthetic input does NOT provide one (the same reason 041 could not prove
audibility headlessly). So the probe can verify the button exists, is reachable and is out of the way,
and that the wiring calls the API — but the fullscreen transition itself is confirmed by the
operator's real tap on their phone, exactly like the IME's soft keyboard.

Also note in the report, as a finding rather than a fix: the port never APPLIES the fullscreen config
anywhere (a grep finds it only in `oh-config`), whereas the C++ applies it at window creation. That is
a pre-existing desktop gap, out of this lane's scope — say so and leave it, unless honoring it on the
web (which is in scope) makes the desktop half a one-liner.

## Report

**Landings (two commits, the task's order):** F32 IME text entry = `fe6507c1` (landed first, the
blocking item), F33 fullscreen button = `918163d4` (second, own register row). Both rows in
`.agents/plans/001-rust-port/deviations.md` (F32, F33 — the F33 row carries the task's FINDING).

### F32 — the web's IME text entry (the soft keyboard's profile-name entry)

The phone browser's soft keyboard now types the profile name on the `ETLPNewBoot` screen through
ONE path: the hidden-but-focusable `#ime` input's `input` event is the sole text path while it is
focused; the keydown listener pushes no `TextEntered` and no `preventDefault` while it is focused
(the browser's own insertion/deletion must run). Four cases, all host-tested pure functions
(`WebImeFeed` routing: printable `data` → `TextEntered`, `deleteContentBackward` → exactly one
`'\u{8}'`, during-composition `input` suppressed, the `compositionend.data` commit fed exactly once
— the double-commit guard) + the `web_keydown_ime_policy` gate + the `BatchPresses`
fast-press-visibility fix (the rAF drain's down+up-inside-the-interval collapse; the C++'s
uncapped-spin `poll()` visibility at the rAF cadence — the winit path needs no twin). Focus/blur
on the `is_entering_text` transition edge via the existing `SoftInput` seam (the 025f
`ImeController`'s web twin — the `WebIme` in `oh-app`).

### F33 — the fullscreen button (the `#fs`)

Bottom-right, 44×44px, subtle/translucent, `pointer-events: auto` on itself only, `display: none`
by default. Click toggles the ported `fullscreen` config value — the SAME shared
`Arc<RwLock<Config>>` the LAlt+Enter bind's `MenuConfig` seam writes through (the button and the
menu's own toggle cannot disagree) — and applies it through the browser Fullscreen API on the
`documentElement` (the `documentElement` so the button stays visible IN the fullscreen). The
`fullscreenchange`'s declarative half is the `html:fullscreen #fs` CSS flip (the Esc / Android
back exit brightens the button, no JS listener). VISIBILITY GATE: the port dispatches the
`ohstatechange` DOM event on the menu↔game transition EDGE (the rAF closure's `WEB_STATE` store
became the edge-detect — the port's house pattern: edge, never per frame, zero lag — NOT a
page-side poll); the shell's gate shows the button in the menu states and HIDES it (`display: none`
— not merely transparent: a hidden element is the only variant that cannot swallow a touch) in the
game state, where the gameplay touch overlay's RotateCW owns that corner.

**FINDING (recorded, not fixed — the task's direction):** the port never applies the `fullscreen`
config anywhere. The winit window is created windowed (`WindowAttributes::new()`, no fullscreen
attribute); `Window::set_fullscreen` is bookkeeping-only; the config's value is read/written by the
LAlt+Enter toggle and the options menu WITHOUT effect. The C++ applied it at the window's
creation/recreation (`Config::setFullscreen`'s `recreateWindow`,
`fullscreen ? sf::Style::Fullscreen : sf::Style::Default` — GameWindow.hpp:90; the toggles at
main.cpp:306, MenuGame.cpp:993, MenuGame.cpp:1371-1373; the `LinkedValue` at Config.hpp:38 /
Config.cpp:321; `setFullscreen` at Config.cpp:639-649 — all re-verified in `cpp-original/`). The F33
callback is the FIRST path that applies the config — on the web target. Fixing the DESKTOP's
non-application is separate work (the winit window's recreation), out of this task's scope.

### What the probe proved (headless CDP — everything short of the operator's phone)

`node tools/web-probe.mjs <url> 30 --seq=...` (the `--seq` machine-speed sequence; the `--phone`
390×844@3 re-run). All of the following observed:

- **Profile creation end-to-end** (the task's acceptance): `Enter,Enter` (LoadingScreen →
  EpilepsyWarning → `ETLPNewBoot`), `Input.insertText "046"` (a genuine IME commit, not a
  keystroke) + a real `C` keypress (the double-typing gate's proof: exactly one char, not two), a
  `Backspace` (the soft-keyboard's `deleteContentBackward` `input` event — the `#ime` value went
  `046C → 046 → 046C` through the re-type), then a FAST machine-speed down+up `Enter` (the
  `BatchPresses`'s worst case) → the game received the name AND created the profile (the state
  transitioned `menu:ETLPNewBoot → menu:SMain` — that transition only fires on a non-empty
  entered name; the game box's screenshot shows exactly `046C`).
- **Focus discipline**: `document.activeElement` = `INPUT:ime` exactly during the text-entry state
  (focused on the `EpilepsyWarning → ETLPNewBoot` edge), `BODY` everywhere else (blurred on the
  `ETLPNewBoot → SMain` exit edge — no stray focused input swallowing the game's keys).
- **The synthetic composition** (the `e64:` base64 dispatch, the phone `--phone` run): a
  compositionstart + `insertCompositionText` + compositionend + the post-commit `insertText` tail
  committed the name EXACTLY once (no double, no corruption).
- **The F33 hit-test property (the task's acceptance)**: in the game state (LevelSelection →
  Enter → `game`), `document.elementFromPoint` at the RotateCW center returns the CANVAS
  (`rotateCwHit: "c"`) — nothing at that corner intercepts the right-thumb rotation — while `#fs`
  is `display: none`; the `ohstatechange` edge fired exactly once on the transition (the DOM
  keylog's line), not per frame. In the menu states the button is `display: block`, 44×44, 12px
  from the bottom-right corner, and hit-tests to ITSELF (`selfHit: "fs"` — clickable).
- **The F33 toggle wiring (both directions)**: a synthetic `#fs.click()` → the
  `[oh-web-fs] fullscreen -> true (browser: requestFullscreen() called …)` console line (the
  config toggle + the API call); a second click → `[oh-web-fs] fullscreen -> false (browser:
  exitFullscreen() called (fullscreen_enabled=true))`.
- **A real fullscreen transition** (when the headless CDP grants the transient user activation —
  observed run-dependent: an immediate click after CDP key inputs): `document.fullscreenElement`
  set, the `html:fullscreen` CSS flip (the button's opacity 0.55→0.8), and the canvas's real-size
  path handled the fullscreen viewport change (the 025g class: the backing store re-sized
  1280→800×800, the `backing == css × dpr` invariant held); the game-state hit-test property held
  in the fullscreen size too. When the CDP does NOT grant the activation, the `requestFullscreen`
  Promise rejects (`TypeError: Permissions check failed` console line) — the task's honest limit,
  now observable either way.
- **The desktop is unregressed**: the desktop (blurred `#ime`) keydown path is byte-identical to
  issue 32 (the `web_keydown_ime_policy`'s desktop-blurred arm, host-tested); the host gate stayed
  green (5/5: fmt, build, clippy `-D warnings`, workspace tests, lua-parity) before each commit.

### What remains unproven without the operator's phone (the last word)

1. **The soft keyboard itself cannot be raised headlessly** (no CDP API for it). The operator's
   phone test: open the web build in Chrome on the phone, press any key through the boot to
   `ETLPNewBoot` (the "new profile" screen), and TAP the name field area — the Android soft
   keyboard should RAISE (the `#ime`'s `focus` on the `is_entering_text` edge), and typing a name +
   the keyboard's "Done"/enter should create the profile and land on `SMain`. The probe verified
   every layer short of the keyboard's own raise (the focus, the `input`-event path, the commit,
   the profile creation); the keyboard's raise is the OS's half of the `focus()` call.
2. **The `requestFullscreen` transient user activation** is granted by a real tap (the operator's
   finger) and flaky headlessly. The operator's tap on the bottom-right `⛶` button should:
   fullscreen the page (the canvas fills the screen, the button brightens — the `html:fullscreen`
   flip), and a second tap (or Esc / the Android back) exits (the button returns to its subtle
   look, the config value toggled each time — the single source of truth).
3. **The desktop's `fullscreen` config non-application** (the FINDING) is recorded, not fixed — a
   separate item (the winit window's recreation).

### Boundaries respected

Web only: `crates/oh-web/` (the `#ime` + `#fs` shell + the gate JS), the `windowed` edge hooks
(`oh-app/src/windowed.rs` — the `WebIme` `SoftInput`, the click sink, the `ohstatechange` edge),
`crates/oh-render/src/window.rs` (the `#ime` / `#fs` get-or-create, the listeners, the
`BatchPresses`, the sinks), `crates/oh-app/Cargo.toml` + `crates/oh-render/Cargo.toml` (the
emscripten `web-sys` features), `tools/web-probe.mjs` (the probe's `--seq` / `--phone` /
`#ime`+`#fs` observables). The Android path is UNTOUCHED (025f works — the operator is comparing
against it). No C++-faithful semantics changed for non-text states. The wasm built in the emsdk
podman container; the host gate green before each commit; explicit-path staging (no `git add -A`);
every process `timeout`-bounded.
