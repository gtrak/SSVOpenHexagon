# Task 041 — make the WASM build produce sound (separate from the Android music bug)

Read `.agents/WORKER-HYGIENE.md` first. Operator directive, verbatim: *"android blips and death
play, and fix the wasm sound separately."* — i.e. the Android music bug is lane 039's and must
not be mixed into this; this lane is the **web** audio.

## The measured starting point (do not re-derive it)

- The operator tried the served wasm build (`target/package/web/_WEB`, served locally on 8081)
  and hears **nothing**.
- `crates/oh-render/Cargo.toml:27` and `crates/oh-audio/Cargo.toml:19` pull **`rodio = "0.23"`
  unconditionally** (rodio's backend is cpal). Nothing in the port gates audio by target.
- `crates/oh-render/src/audio.rs` is the device seam: `AudioDevice { sink:
  Option<rodio::MixerDeviceSink> }` with a no-op headless variant when no sink opens
  (`:390-406`), and `crates/oh-audio/src/lib.rs:116-125` already handles a failed open:
  `eprintln!("[oh-audio] no audio device — headless: {e:#}")`.
- **That line is now visible to a human**: issue 026's log sink routes the port's own lines to
  the browser console, so the console says which case you are in.

## Step 1 — DIAGNOSE, do not guess (this decides the whole shape of the fix)

Load the served page in a browser and read the console (and, if you drive it headlessly, capture
the console over CDP). There are exactly two worlds:

**(a) The device FAILS to open** (you see `no audio device — headless: …`, or the sink is `None`).
Then the question is whether cpal supports this target at all: our web target is
`wasm32-unknown-emscripten`, while cpal's browser backend is historically gated on
`target_os = "unknown"` (the wasm-bindgen path), not emscripten. Establish the fact from the
dependency tree and cpal's own cfg/build script before writing code — if cpal cannot serve
emscripten, the fix is a **sink implementation behind the EXISTING seam** (below), not a config
flag.

**(b) The device OPENS but is silent.** Then the overwhelmingly likely cause is the **browser
autoplay policy**: a WebAudio `AudioContext` is created *suspended* and only starts producing
sound after a user gesture resumes it. The port never resumes it, so it plays into a suspended
context: state `suspended` forever, silence, no error. The fix is to resume the context on the
first user interaction — the port already has the input path for that (issue 031/032's input
state), so it is a small, well-placed hook rather than an audio rewrite.

## Step 2 — fix it, in the shape the diagnosis dictates

- **Prefer keeping the seam.** `oh-render::audio`'s `AudioDevice` is the boundary the engine
  talks to; a web implementation (a `web-sys` `AudioContext` + a mixer/source-node sink, with
  the same `play`/`stop`/volume semantics SFML's `sf::Sound`/`sf::Music` model has) belongs
  behind it, exactly as 025e put `rodio` there for desktop/Android. **Do not** fork the engine's
  audio logic, and do not make the web path a set of `eprintln!` no-ops.
- **The decode is already portable**: the OGG decoding the port uses is pure Rust (the SFX path
  decodes to in-memory buffers; music streams), so the bytes can come from the preloaded MEMFS
  tree exactly as they do for the desktop build — no new decoder needed. Verify that claim
  rather than assuming it.
- **Keep the graceful no-op.** A headless/no-device environment (CI, a browser with audio
  blocked) must still build, boot and tick — the current degradation is a feature, and the
  browser-boot acceptance of issue 026 must keep passing.
- **Music AND one-shots**, since the operator's report is about the game being silent: the
  acceptance is the game's own sounds (the menu blips and the level music are the two easiest
  tells — the same ones that made 039's diagnosis conclusive on Android).

## Step 3 — verify in a real browser, with both kinds of evidence

- **Programmatic**: the `AudioContext` exists and reaches state `running` after a gesture; a
  source is actually playing (the context's `currentTime` advances, or a source node reports
  playing). Capture this over CDP so it is evidence rather than an impression.
- **The operator's ear**: the human check is theirs, and this lane's page is already being served
  at `http://localhost:8081/` (and `http://192.168.10.31:8081/`). Say in the report exactly how
  they can re-test, and note that a **page reload** or a **fresh build served on a new port** may
  be needed so they do not judge a cached page.
- **The build must stay green**: the same emsdk container recipe
  (`tools/package-web.sh`; `CXXFLAGS_wasm32_unknown_emscripten=-fwasm-exceptions`; the root
  `.cargo/config.toml` carries the rest), the host gate, and the wasm CI job that `.github/
  workflows/ci.yml` runs. If the browser needs a gesture, document it — an audio path that needs
  one tap to start is normal on the web, but a silent first click would look like a bug.

## Hygiene

One commit per milestone (diagnosis findings as a report section, then the fix, then the
verification), explicit paths, `cargo fmt`, bound every process with `timeout`, at most ONE image
read (you probably need none — console/CDP text is the evidence here), and **stop-and-report** if
the diagnosis shows the fix is a large audio backend rather than a hook, with the cost stated.
Do not touch lane 039's Android work, do not change desktop/Android audio behaviour, and do not
weaken the web release/CI wiring lane 040 owns.

## The orchestrator's prior (VERIFY it — the console and the dependency tree win over this)

Three things I checked before dispatching, so you do not spend a cycle rediscovering them:

1. **Expect world (a) — and expect it to be cpal's cfg, not the port's code.** cpal's browser
   backend is historically gated on `target_os = "unknown"` (the wasm-bindgen path, i.e.
   `wasm32-unknown-unknown`), while our web target is `wasm32-unknown-emscripten`. Establish the
   fact from `cargo tree -p cpal --target wasm32-unknown-emscripten` and cpal's own cfg/build
   script, then STATE which world you measured — the fix's shape depends entirely on it, and if it
   is (a) the fix must live behind the existing seam (`crates/oh-render/src/audio.rs`'s
   `AudioDevice`), never in a config flag.
2. **The parity-faithful sink is OpenAL, because that is what the C++ actually uses.** SFML's audio
   module is OpenAL, and emscripten ships an OpenAL implementation (`-lopenal`) that maps onto Web
   Audio. So an emscripten OpenAL sink behind the seam mirrors the original's audio stack — while
   rodio/cpal is itself the port's deviation. Keep the SFML-unit volume mapping the desktop path
   already implements (039's device evidence: 100/100 SFML units through to rodio 1.0) so the two
   paths agree on numbers.
3. **Browsers require a user gesture before audio starts.** The port should resume/unlock on the
   first input (032's web input path already exists), and your diagnostic must distinguish "no
   device" from "device present but suspended" — that distinction is the whole answer to the
   operator's "there appears to be no sound there".

SCOPE GUARD: the Android music/content question is CLOSED (039, supervisor decision A: the shipped
_RELEASE has no music OGGs, upstream-faithful). Do not mix content into this lane. This lane is the
web audio MECHANISM.

ACCEPTANCE: `tools/web-probe.mjs` against a local serve of your build must show your new diagnostic
lines (world (a) vs (b), and the unlock state), and — if you can produce it — an audible claim backed
by an observable (Web Audio node/graph state, a decoded-buffer assertion, or an emscripten OpenAL
error/source count). If headless cannot prove audibility, SAY SO and give the strongest observable
you have rather than claiming sound. One image read allowed if you need it; bound everything.
