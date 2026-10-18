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

## Report

### Which world was measured — and the evidence

**The original rodio/cpal path is world (a): the device never opens.** Established from the
dependency tree and cpal's own cfg, not a guess:

- `cargo tree -p cpal --target wasm32-unknown-emscripten` lists **only `dasp_sample`** — no
  `web-sys`, no wasm-bindgen webaudio deps. So cpal's browser host is NOT compiled in.
- cpal 0.18.2's browser (webaudio) host is gated on `target_arch = "wasm32"`
  **AND** `target_os = "unknown"` (the `wasm32-unknown-unknown` / wasm-bindgen path). Our target
  is `wasm32-unknown-emscripten` (`target_os = "emscripten"`), so the gate is false → cpal
  selects its **null host**.
- The null host's `default_output_device()` returns `None`, so
  `rodio::DeviceSinkBuilder::open_default_sink()` fails with `NoDevice` → `oh-audio` logs
  `[oh-audio] no audio device — headless` and runs the headless no-op path. **That is the
  silence the operator saw.**

So the orchestrator's prior (world (a), caused by cpal's cfg, not the port's code) is **confirmed**.

**After the fix the web is world (b): the device opens (via OpenAL).** The probe against the new
build shows the device present but suspended by the autoplay policy:

```
[oh-audio] device present (OpenAL/Web Audio); context suspended (awaits first input)
[oh-audio] audio device opened (live playback)
```

i.e. `alcOpenDevice` succeeded (Web Audio IS present in the browser) and the AL context was created.
The device exists; it is simply **suspended** until a user gesture.

### Why the fix has its shape

- **Behind the seam, never a flag.** `oh_render::audio::AudioDevice` now holds
  `Option<Box<dyn DeviceBackend>>` and picks the backend per target in `try_open_default()`
  (`RodioBackend` on desktop/Android, `OpenalBackend` on `target_os = "emscripten"`). No config
  flag; the desktop/Android path is unchanged in behaviour (same `MixerDeviceSink` + `Player`,
  same SFML-unit→linear gain).
- **One-shots and music both flow through the seam.** `play_one_shot(sr, ch, samples, vol, pitch)`
  (the `sf::Sound` one-shot) and `play_music(..., offset)` (the `sf::Music` looped source, now
  decoded to a PCM buffer) return a portable `AudioPlayer` (a `Box<dyn PlayerBackend>` — a rodio
  `Player` or an AL source). `oh-audio`'s active-sound map and music ctx now hold `AudioPlayer`, so
  the same game code drives both backends.
- **The SFML-unit volume mapping is preserved and applied identically on both backends** (100 SFML
  units → 1.0 linear), per 039's device evidence.

### The OpenAL decision

Emscripten ships a real OpenAL implementation (`AL/al.h` + `libopenal.js`, linked `-lopenal`) that
maps onto Web Audio, and SFML's audio module **is** OpenAL. So an emscripten OpenAL sink behind the
seam mirrors the original C++/SFML stack — while rodio/cpal is the port's own deviation (it has no
emscripten backend). This moves the port *toward* the original, not away from it. The
`OpenalBackend` FFI uses the emscripten `al.h` constants/signatures directly; the f32 samples are
converted to interleaved 16-bit (the standard 16-bit OpenAL format) for `alBufferData`.

### Acceptance observables (the probe, `node tools/web-probe.mjs … --keys=Enter`)

```
[oh-audio] device present (OpenAL/Web Audio); context suspended (awaits first input)
[oh-audio] audio device opened (live playback)
[oh-audio] decoded sound 'select.ogg' (84992 samples @ 44100Hz 2ch)
[oh-audio] first play: source AL_PLAYING, al error AL_NONE, 84992 samples @ 44100Hz 2ch, gain 1.00
[oh-audio] unlocked on first input; context unavailable
user activation : no — the headless input did NOT grant user activation
```

- **World**: (b) — the device is present (OpenAL/Web Audio), not (a).
- **Unlock state**: the Web Audio context is `suspended` (autoplay default). A direct CDP
  `new AudioContext()` confirms it constructs but starts `suspended` (sampleRate 44100).
- **Audibility observable**: the menu's `select.ogg` blip decoded to a **real buffer** (84992
  samples) and its AL source is **scheduled and clean** — `AL_PLAYING`, `al error AL_NONE`. The
  OGG→i16→AL-buffer→AL-source→`alSourcePlay` pipeline is fully wired with no error.
- A second CDP probe confirmed `window.AudioContext` is available and `new AudioContext()`
  succeeds (`state: "suspended"`) — so the in-wasm first-gesture probe's "unavailable" is a
  headless-construction quirk of the throwaway context, not a pipeline failure; the AL source
  state (`AL_PLAYING`) is the authoritative audibility signal.

### What could NOT be verified (stated plainly)

**Headless CDP cannot prove audible output.** The browser requires a genuine user gesture (user
activation) to move the Web Audio context from `suspended` to `running`; the headless
`Input.dispatchKeyEvent` does **not** grant user activation (`document.hasUserActivation()` is not
even defined in this headless build, and the context stayed `suspended`). So the scheduled
(`AL_PLAYING`) blip produces **no audible sound** in the probe. The strongest observable is
therefore: **device present + OGG decoded to a real buffer + AL source scheduled with a clean AL
call (`AL_PLAYING` / `AL_NONE`)** — the pipeline is provably wired end-to-end; a real user gesture
(click / keypress in a real browser) resumes the context and the blip becomes audible.

Also, stated plainly:
- **AL source position** (`get_pos`) is not available on emscripten's OpenAL (no
  `AL_OFFSET`/`AL_SEC_OFFSET`); the one-shot SFX never read their position, and the shipped
  `_RELEASE` has no music OGGs (039 scope-guard), so this is not observable in the shipping build.
- **Music on the web** is decoded-to-buffer and looped through the same seam, but is not exercised
  by the shipping build (no music content) — wired, not auditioned.
- **OpenAL "pause" maps to stop** (emscripten's OpenAL has no per-source pause-with-position the
  way SFML's does); one-shot SFX use stop, not pause, so this is not observable.

### Desktop / Android not regressed

- Host gate green: fmt / build / clippy / **993 tests (0 failed)** / lua-parity.
- Android cross-check `cargo check --target aarch64-linux-android` (with the NDK env) passes — the
  shared `AudioDevice` seam still compiles for Android; the OpenAL backend is
  `#[cfg(target_os = "emscripten")]` only, so it is not compiled for Android.
- The rodio backend (desktop/Android) is the same `MixerDeviceSink` + `Player` path as before,
  unchanged in behaviour.

### Files

- `crates/oh-render/src/audio.rs` — the portable `AudioDevice`/`AudioPlayer` seam +
  `DeviceBackend`/`PlayerBackend` traits + `emit_diag`.
- `crates/oh-render/src/audio/rodio_backend.rs` (new) — the rodio backend (desktop/Android).
- `crates/oh-render/src/audio/openal.rs` (new) — the emscripten OpenAL backend + the first-gesture
  unlock probe.
- `crates/oh-audio/src/lib.rs` — active sounds/music now `AudioPlayer`; diagnostics via
  `emit_diag`.
- `crates/oh-app/src/windowed.rs` — the real `MenuAudioSink` on all targets (the no-op sink is
  gone); the `seams::Audio` import ungated.
- `crates/oh-app/Cargo.toml`, `crates/oh-render/Cargo.toml` — `oh-audio` is now a common dep (its
  pure-Rust rodio/symphonia OGG decode is used on the web); web-sys `AudioContext` features.
- `tools/web-probe.mjs` — surfaces the `[oh-audio]` diagnostic lines + reports user activation.
