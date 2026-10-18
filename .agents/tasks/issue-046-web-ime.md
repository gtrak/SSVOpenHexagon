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
