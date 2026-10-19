#!/usr/bin/env node
// tools/web-probe.mjs — drive the wasm build in a real browser over CDP and report what it does.
//
// Why this exists: the wasm target's acceptance used to be ad-hoc (a throwaway driver, a screenshot
// read by eye). This is the durable version — no npm dependencies (node >=22 has a global
// WebSocket), no bundler: it launches headless Chrome with SwiftShader, navigates, optionally
// presses keys, and prints the port's own state timeline, every non-200 response, every console
// error, and a screenshot.
//
//   node tools/web-probe.mjs http://127.0.0.1:8080/ 30            # boot, watch, screenshot
//   node tools/web-probe.mjs https://gtrak.github.io/SSVOpenHexagon/ 45 --keys=Enter,s
//   node tools/web-probe.mjs http://127.0.0.1:8080/ 60 --phone \
//     --seq=Enter,Enter,i:046,C,Enter   # issue 046: the IME path — the boot
//     screens' Enters reach the name entry (the `#ime` focus), `i:` is the
//     CDP `Input.insertText` (a genuine IME-style commit, not a keystroke),
//     `C` is a REAL keypress INTO the focused input (the double-typing gate
//     proof: one char, not two), the last Enter CREATES the profile (the
//     C++'s `okAction` bind — the 025d/025f device flow's observable);
//     each step reports the state + the focus + the `#ime` value.
//   node tools/web-probe.mjs http://127.0.0.1:8080/ 120 --phone \
//     --shots=.agents/evidence/054 \
//     --seq=t:ok;60,s:01-menu,t:menu;150,s:02-menu-press \
//     # issue 054: the touch surface — `t:X;Y;DUR` is a CDP
//     `Input.dispatchTouchEvent` HELD touch (touchStart → DUR ms →
//     touchEnd — the real touch path: the mobile-emulated browser
//     synthesizes the pointer events the canvas's pointer feed rides on,
//     exactly as a finger on the operator's phone would), X,Y in the
//     PORT'S view-pixel space (the button-centers / grid-thirds space;
//     the named presets `menu` / `ok` / `left` / `right` / `up` /
//     `down` are the 054 evidence's vocabulary) and `s:NAME` captures a
//     screenshot to `--shots/NAME.png` (the per-step evidence — the
//     final screenshot's `--shot` sibling).
//   node tools/web-probe.mjs http://127.0.0.1:8080/ 60 --phone \
//     --expect-cm \
//     --seq=Enter,Enter,cm,t:menu;900,cm:ime,hc:Enter;2000 \
//     # issue 058 (F37): the browser context-menu's suppression — the
//     two boot Enters reach the name entry (the `#ime`'s focus), `cm`
//     is the CDP RIGHT-CLICK at the canvas's center (the page's
//     `contextmenu` event — the shell's F37 `preventDefault`'s
//     `defaultPrevented` is the step check's dom log's `cm:<target>/
//     prev:<bool>` entry), `t:menu;900` is the LONG-PRESS (the mobile
//     context menu's trigger — the 900 ms hold's browser synthesis —
//     the `menu` corner: the name entry's up zone, a no-state-change
//     hold, unlike the `ok` center's profile-create),
//     `cm:ime` right-clicks the #ime input (the EXCLUSION: its `prev`
//     must be false — the desktop's paste / copy affordance stays),
//     and `hc:KEY;MS` HOLDS the key with a right-click at the 50% mark
//     (the held-input focus-loss probe: the focus counters' blur half
//     say whether the context menu ever took the window's focus — a
//     headless context menu that does NOT take focus is the honest
//     negative; the suppression's defaultPrevented half is the
//     measurement either way). `--expect-cm` gates the exit code on the
//     verdict (a canvas contextmenu prevented + no #ime one).
//
// The port logs `status booted=…;exit_code=…;ticks=…;state=menu:<States|game>` through its
// log_sink (oh-web installs a console.log sink), so the state timeline below is the port's own
// account of itself — not an inference from pixels. issue 041 also surfaces the port's
// `[oh-audio]` diagnostics (which world the web audio device is in + the unlock / first-play
// observables) and whether the headless CDP input granted user activation (the gate on the Web
// Audio context running).
//
// Exit: 0 when the module reported booted=true and no console errors occurred (a missing
// /favicon.ico is tolerated: GitHub Pages serves no favicon and it is not the game's asset);
// 1 otherwise. That makes it usable as a manual gate stage on a machine with a browser.

import { spawn } from 'node:child_process';
import { setTimeout as sleep } from 'node:timers/promises';
import { writeFileSync, existsSync, mkdirSync } from 'node:fs';

const argv = process.argv.slice(2);
const url = argv.find((a) => !a.startsWith('--')) || 'http://127.0.0.1:8080/';
const positional = argv.filter((a) => !a.startsWith('--'));
const secs = Number(positional[1] || 30);
const keysArg = (argv.find((a) => a.startsWith('--keys=')) || '').slice(7);
const keys = keysArg ? keysArg.split(',') : [];
// issue 046 — the IME / fullscreen verification surface:
// * `--seq=A,B,C` — a per-step ACTION script (replaces `--keys` when
//   given): a bare item is a keypress (`Enter` / `s` / `Backspace` /
//   any single char — the CDP `Input.dispatchKeyEvent`'s keyDown+keyUp,
//   the REAL browser input path: the focused element's own editing runs,
//   so a key into the focused `#ime` input fires the `input` event the
//   IME feed rides on); `i:TEXT` is the CDP `Input.insertText` (a
//   genuine IME-style COMMIT into the focused element — NOT a
//   keystroke, the task's "prove the IME path programmatically");
//   `e:JS` is a `Runtime.evaluate` (the synthetic composition /
//   backspace event dispatches — the headless stand-ins for the soft
//   keyboard's composition flows); `t:X;Y;DUR` is a CDP
//   `Input.dispatchTouchEvent` HELD touch (touchStart → DUR ms →
//   touchEnd — the issue 054 touch surface's driver: the held-press
//   cadence's operator-visible half, the F35 row's evidence), X,Y in
//   the port's view-pixel space (÷ the DPR to the CDP's CSS px — the
//   headless viewport is NOT the emulated size, so the probe measures
//   the canvas + DPR at run time; the named presets `t:menu;DUR` /
//   `t:ok;DUR` / `t:left;DUR` / `t:right;DUR` / `t:up;DUR` /
//   `t:down;DUR` are the grid's zone centers + the MENU corner — the
//   `;` keeps the triple off the seq's `,` delimiter); `m:BTN;MS`
//   holds the mouse button (CDP `Input.dispatchMouseEvent` press at the
//   canvas center for MS ms, then release — the issue 057 held-input
//   reproduction's mouse half; optional third field = a mid-hold
//   screenshot name); `mv:X;Y` moves the mouse to X;Y (the CSS viewport's
//   coords — the menu's hover hit-test's positioning, the port's
//   `MouseMoved` fold's + the 29u1 cursor re-show's source); `mc:X;Y[;MS]`
//   clicks (presses + holds + releases) at X;Y — the issue 058 click
//   acceptance's driver (the menu item's click-to-activate: NO implicit
//   move — the port's `pointerdown` fold moves the mouse to the press's
//   position itself, so the move-first case is the `mv:` + `mc:` pair);
//   `s:NAME`
//   captures a screenshot to `--shots/NAME.png` (the per-step
//   evidence; `--shots` absent → the `--shot` path's directory).
// * `--phone` — the emulated phone viewport (the
//   `Emulation.setDeviceMetricsOverride` 390x844 @3 mobile + touch —
//   the operator's Android Chrome's shape; the soft keyboard itself
//   cannot be raised headlessly — the report says so plainly).
// * per-step — after EVERY seq action the probe reads the page's own
//   observables (the port's `status()`'s state via the page's
//   `window.ohStatus` handle, `document.activeElement` — the
//   "focused exactly while a text-entry state, blurred otherwise"
//   acceptance — the `#ime` input's value — the entered-text evidence —
//   + the canvas's real-size invariant, the 025g bug class's guard).
const seqArg = (argv.find((a) => a.startsWith('--seq=')) || '').slice(6);
const seq = seqArg ? seqArg.split(',').filter(Boolean) : [];
const phone = argv.includes('--phone');
const shotPath = (argv.find((a) => a.startsWith('--shot=')) || '').slice(7) || '/tmp/web-probe.png';
const shotsDir = (argv.find((a) => a.startsWith('--shots=')) || '').slice(8)
  || shotPath.replace(/\/[^/]*$/, '');

const CHROME = process.env.OH_CHROME
  || ['/home/gary/.agent-browser/browsers/chrome-148.0.7778.97/chrome',
      '/usr/bin/google-chrome', '/usr/bin/chromium'].find((p) => existsSync(p));
if (!CHROME) { console.error('web-probe: no chrome found; set OH_CHROME=/path/to/chrome'); process.exit(1); }
// The CDP port is PER-PID (the lane-058 follow-up's finding: the fixed
// 9335 let a probe run silently drive a STALE Chrome left behind by an
// earlier run (its `timeout`-killed node never ran the kill) — the
// stale Chrome's own user-data-dir / IDBFS then answered the run, and
// its page state (an old profile, an old SMain) was measured as if it
// were fresh. A per-PID port makes the health-check's owner the
// probe's own Chrome: a stale instance holds its own, different port.)
const PORT = Number(process.env.OH_CDP_PORT || (10000 + (process.pid % 20000)));

// OH_CHROME_EXTRA: extra Chrome args, `;`-separated (the lane-058
// follow-up's headless artifact: without `--ozone-override-screen-size`
// the SwiftShader compositor drops the mid-screen menu quads out of the
// screenshot (they render black); the flag forces the large emulated
// viewport's GL surface to composite. It is a MEASUREMENT-harness knob,
// NOT a page or port setting — a real browser / Android never needs it).
const chrome = spawn(CHROME, ['--headless=new', `--remote-debugging-port=${PORT}`, '--no-sandbox',
  '--disable-dev-shm-usage', '--use-angle=swiftshader', '--enable-unsafe-swiftshader',
  `--window-size=${process.env.OH_WINDOW_SIZE || '1280,720'}`, '--no-first-run', `--user-data-dir=/tmp/oh-web-probe-profile-${process.pid}`, 'about:blank',
  ...(process.env.OH_CHROME_EXTRA ? process.env.OH_CHROME_EXTRA.split(';').map((a) => a.trim()).filter(Boolean) : [])],
  { stdio: ['ignore', 'ignore', 'ignore'] });
// The Chrome's death on ANY probe exit (the lane-058 follow-up's
// companion to the per-PID port): a `timeout`'d run's SIGTERM used to
// kill the node and orphan the Chrome (which then owned the CDP port
// for the next run). The 'exit'/'SIGTERM'/'SIGINT' handlers cover the
// node-side deaths; the SIGKILL (timeout -k) orphans no handler can
// catch — the per-PID port bounds that case's damage to the dead
// port, not to the next run's measurement.
process.on('SIGTERM', () => { try { chrome.kill('SIGKILL'); } catch { /* already dead */ } process.exit(143); });
process.on('SIGINT', () => { try { chrome.kill('SIGKILL'); } catch { /* already dead */ } process.exit(130); });
process.on('exit', () => { try { chrome.kill('SIGKILL'); } catch { /* already dead */ } });

const http = async (p, i) => (await fetch(`http://127.0.0.1:${PORT}${p}`, i)).json();
let up = false;
for (let i = 0; i < 60 && !up; i++) { try { await http('/json/version'); up = true; } catch { await sleep(500); } }
if (!up) { chrome.kill('SIGKILL'); console.error('web-probe: chrome never opened its debug port'); process.exit(1); }

const target = await http(`/json/new?${encodeURIComponent(url)}`, { method: 'PUT' });
const ws = new WebSocket(target.webSocketDebuggerUrl);
let id = 0; const pending = new Map();
const logs = []; const errors = []; const bad = []; const failed = [];
ws.addEventListener('message', (ev) => {
  const m = JSON.parse(ev.data);
  if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); return; }
  const p = m.params || {};
  if (m.method === 'Runtime.consoleAPICalled') {
    const text = (p.args || []).map((a) => a.value ?? a.description ?? '').join(' ');
    if (p.type === 'error' || p.type === 'warning') errors.push(`[${p.type}] ${text}`); else logs.push(text);
  } else if (m.method === 'Runtime.exceptionThrown') {
    errors.push(`[exception] ${p.exceptionDetails?.exception?.description || p.exceptionDetails?.text}`);
  } else if (m.method === 'Network.responseReceived') {
    if (p.response.status >= 400) bad.push(`${p.response.status} ${p.response.url}`);
  } else if (m.method === 'Network.loadingFailed') {
    failed.push(`${p.errorText} (${p.type})`);
  }
});
const send = (method, params = {}) => new Promise((r) => { const i = ++id; pending.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
await new Promise((r) => ws.addEventListener('open', r));
await send('Runtime.enable'); await send('Network.enable'); await send('Page.enable');
// A REUSED profile plus a CDN/Pages cache makes a freshly-deployed build look unchanged: the
// orchestrator hit exactly that (two byte-identical screenshots of a static screen, while the
// deployed .wasm had in fact changed by 578 bytes). Disable the cache outright and use a per-run
// profile so the probe always measures what the server is serving right now.
await send('Network.setCacheDisabled', { cacheDisabled: true });
// issue 046 — the emulated phone viewport (the operator's Android
// Chrome's shape: 390x844 @3, mobile = the touch emulation). The soft
// keyboard itself cannot be raised headlessly (the report says so
// plainly) — this is the viewport / touch half of the phone re-run.
if (phone) {
  // The emulated phone viewport (the operator's Android Chrome's
  // shape: 390x844 @3, mobile = the touch emulation). The soft keyboard
  // itself cannot be raised headlessly (the report says so plainly).
  // NOTE (the 054 run's finding): this headless shell SCALES the
  // override's screen size (×~2.5: the 390×844 screen comes out a
  // 980×2121 LAYOUT viewport @3), so the convergence below (after the
  // navigate) re-applies the override with the MEASURED viewport —
  // the 054 run's verified fixed point (980×2121 stays 980×2121) —
  // making the canvas's CSS box == the viewport: the screenshot's
  // device px == the canvas's backing px == the game view's pixels
  // (the whole view, no crop; without it the menu's center content
  // draws outside the captured crop).
  await send('Emulation.setDeviceMetricsOverride', {
    width: 390, height: 844, deviceScaleFactor: 3, mobile: true,
  });
  console.log('  … phone viewport (390x844 @3, touch — converging post-load)');
}
await send('Page.navigate', { url });
// issue 046 — the DOM event log (diagnostic): what the browser
// ACTUALLY dispatched (the `keydown` / `keyup` / `input` /
// `composition*` / `focus` / `blur` + the `defaultPrevented` flag —
// whether the page — the Rust's window-capture listener — swallowed
// the event). The probe-side mirror of the Rust listener's view: no
// rebuild needed, and it separates "the browser delivered the event
// to the page" from "the game side consumed it".
{
  // The document must be READY before the listeners exist (the first
  // input must land on them): the page is local (instant), the wasm
  // boot takes seconds (irrelevant — the DOM events do not need it).
  for (let i = 0; i < 40; i++) {
    const r = await send('Runtime.evaluate', { expression: 'document.readyState', returnByValue: true });
    if (r.result?.result?.value === 'complete' || r.result?.result?.value === 'interactive') break;
    await sleep(250);
  }
  await send('Runtime.evaluate', { expression: `(() => {
    window.__keylog = [];
    const rec = (t, d) => { if (window.__keylog.length < 200) window.__keylog.push(t + (d ? ':' + d : '')); };
    window.addEventListener('keydown', (e) => { rec('down', e.code + '/' + e.key + '/comp=' + e.isComposing + '/rep=' + e.repeat + '/prev=' + e.defaultPrevented); }, true);
    window.addEventListener('keyup', (e) => { rec('up', e.code + '/' + e.key); }, true);
    window.addEventListener('input', (e) => { rec('input', (e.target && e.target.id) + '/' + e.inputType + '/' + JSON.stringify(e.data)); }, true);
    window.addEventListener('compositionstart', (e) => { rec('compStart', e.target && e.target.id); }, true);
    window.addEventListener('compositionend', (e) => { rec('compEnd', JSON.stringify(e.data)); }, true);
    window.addEventListener('focus', (e) => { rec('focus', e.target && e.target.id); }, true);
    window.addEventListener('blur', (e) => { rec('blur', e.target && e.target.id); }, true);
    // issue 054 — the CANVAS's pointer events (the touch surface's browser
    // half: the CDP synthetic touches' pointer synthesis is the probe-side
    // mirror of the Rust pointer feed's view — "the browser delivered the
    // touch to the canvas" vs "the game side consumed it", the 046
    // keylog's diagnostic class).
    const c0 = document.getElementById('c');
    if (c0) {
      c0.addEventListener('pointerdown', (e) => { rec('pdown', e.pointerType + '/' + e.pointerId + '/' + Math.round(e.clientX) + ',' + Math.round(e.clientY)); }, true);
      c0.addEventListener('pointerup', (e) => { rec('pup', e.pointerType + '/' + e.pointerId); }, true);
      c0.addEventListener('pointercancel', (e) => { rec('pcancel', e.pointerType + '/' + e.pointerId); }, true);
    }
    // issue 046 (F33) — the port's menu<->game transition EDGE (the
    // ohstatechange DOM event — the house pattern's edge, not a
    // poll): the #fs visibility gate's signal (its display flipping
    // between steps is the gate's observable proof).
    document.addEventListener('ohstatechange', () => { rec('ohstatechange', ''); });
    // issue 057 — the focus / blur / visibility / fullscreen COUNTERS
    // (the held-input reproduction's instrumentation: a spurious
    // window blur or visibilitychange mid-hold would clear the
    // port's LostFocus fold — the counters say whether the DOM ever
    // went there; the port's LostFocus fold is 1:1 with the window
    // blur + the visibilitychange→hidden, so these ARE the fold's
    // input counts). NO capture phase: a capture listener would also
    // count every ELEMENT blur (e.g. the #ime input blurring when the
    // menu advances), which is not a window focus-loss — the bubble-
    // phase window blur/focus are the window's OWN focus change only.
    window.__fc = { blur: 0, focus: 0, vis: 0, fs: 0 };
    window.addEventListener('blur', () => { window.__fc.blur++; });
    window.addEventListener('focus', () => { window.__fc.focus++; });
    document.addEventListener('visibilitychange', () => { window.__fc.vis++; }, true);
    document.addEventListener('fullscreenchange', () => { window.__fc.fs++; }, true);
    // issue 058 (F37) — the contextmenu log: the listener installs
    // LAZILY (the ensureCmListener below, on the first cm / hc: / t:
    // step — AFTER the wasm boot): the page's own listener is
    // registered by the module scope at boot (the await Module(cfg)'s
    // resume), which is AFTER this installer's parse-time run — an
    // installer-time listener would precede the page's and read
    // defaultPrevented before the page's preventDefault ran (the
    // first run's cm:c/prev:false — the probe's error, not the
    // page's). The __cmlog (the final verdict's source) is seeded
    // here so the verdict's read is a no-op-safe shape.
    window.__cmlog = [];
    return 'installed';
  })()` });
}

const stateOf = async () => {
  // issue 046 — the page's `window.ohStatus` handle first (the module
  // scope's `mod` is not a global; the page script exposes it — the
  // per-step state read), the old `Module` global check as the fallback.
  const r = await send('Runtime.evaluate', { expression: `(() => {
    if (typeof window.ohStatus === 'function') { try { return window.ohStatus(); } catch { return null; } }
    return (typeof Module!=='undefined'&&Module.status)?Module.status():null;
  })()`, returnByValue: true });
  return r.result?.result?.value ?? null;
};
// The named-key virtual-key codes (the CDP `windowsVirtualKeyCode` —
// `charCodeAt` covers the single letters, the named keys need the map).
const VK = { Enter: 13, Backspace: 8, Escape: 27, ArrowUp: 38, ArrowDown: 40, ArrowLeft: 37, ArrowRight: 39 };
// issue 057 — the shared screenshot helper (the `s:`'s body, extracted so
// the `h:` / `m:`'s MID-HOLD screenshots can reuse it — the change-detection
// observable's mid-hold stills).
const snap = async (name) => {
  mkdirSync(shotsDir, { recursive: true });
  const p = `${shotsDir}/${name}.png`;
  const r = await send('Page.captureScreenshot', { format: 'png' });
  if (r.result?.data) writeFileSync(p, Buffer.from(r.result.data, 'base64'));
  console.log(`  … screenshot -> ${p}`);
};
// issue 058 (F37) — the contextmenu listener's lazy install (the
// installer block's note above: after the wasm boot's module-scope
// listener, so the page's preventDefault runs first). Pushes to both
// the per-step __keylog (the step check's dom dump) and the __cmlog
// (the final verdict's source — NOT drained by the step checks).
let cmListenerInstalled = false;
const ensureCmListener = async () => {
  if (cmListenerInstalled) return;
  cmListenerInstalled = true;
  await send('Runtime.evaluate', { expression: `(() => {
    if (window.__cmInstalled) return 'already';
    window.__cmInstalled = true;
    document.addEventListener('contextmenu', (e) => {
      const entry = 'cm:' + ((e.target && (e.target.id || e.target.tagName)) || '?') + '/prev:' + e.defaultPrevented;
      if (window.__cmlog) window.__cmlog.push(entry);
      if (window.__keylog && window.__keylog.length < 200) window.__keylog.push(entry);
    });
    return 'installed';
  })()` });
};
const press = async (key) => {
  const vk = VK[key] ?? (key.length === 1 ? key.toUpperCase().charCodeAt(0) : 0);
  const code = key.length === 1 ? `Key${key.toUpperCase()}` : key;
  for (const type of ['keyDown', 'keyUp']) {
    await send('Input.dispatchKeyEvent', { type, key, code, windowsVirtualKeyCode: vk, nativeVirtualKeyCode: vk, text: type === 'keyDown' && key.length === 1 ? key : key === 'Enter' ? '\r' : undefined });
  }
};
// The machine-speed down+up (a REAL tap holds 80–150ms — but the
// machine-speed pair is the WORST case for the game's rAF drain: the
// pair can land in ONE frame's event batch, where the port's
// fast-press visibility (the `BatchPresses`'s docs — the C++'s
// uncapped-spin `poll()`'s poll-inside-the-press) must keep the press
// visible to the frame's update. If the fast pair works, a human tap
// (across several frames) does too.
// issue 046 — the per-step page observables (the docs above): the state
// (the port's own account of itself), the focus (the IME element's
// focus/blur discipline), the `#ime` value (the entered-text evidence),
// the canvas's real-size invariant (the backing store == the CSS box x
// the DPR — the 025g bug class the fullscreen resize would fall into).
const stepCheck = async (label) => {
  const r = await send('Runtime.evaluate', { expression: `(() => {
    const ime = document.getElementById('ime');
    const a = document.activeElement;
    const c = document.getElementById('c');
    const cr = c ? c.getBoundingClientRect() : null;
    const keylog = (window.__keylog || []).splice(0);
    // issue 046 (F33) — the #fs button's observables (the register row's
    // hit-test property): its display (the gate's display:none in the
    // game state — a HIDDEN element, not merely transparent, is the only
    // variant that cannot swallow a touch) + its rect (the ~44px target,
    // the bottom-right corner's offset) + two hit-tests: the button's
    // own center (must be ITSELF when visible — the menu's clickable
    // proof) and the gameplay touch overlay's RotateCW corner (must be
    // the CANVAS while the game runs — nothing at that corner may
    // intercept the right-thumb rotation). The RotateCW center is
    // (realW-44, realH-44) in the overlay's view-pixel space (PAD 12 +
    // half 32 = 44 from each edge); elementFromPoint wants CSS px,
    // so divide by the DPR (the canvas fills the viewport, so its CSS
    // box is the viewport).
    const fs = document.getElementById('fs');
    let fsInfo = null;
    if (fs) {
      const fr = fs.getBoundingClientRect();
      const dpr2 = window.devicePixelRatio || 1;
      const btnCenter = document.elementFromPoint(fr.left + fr.width / 2, fr.top + fr.height / 2);
      const rcx = c ? (c.width - 44) / dpr2 : 0;
      const rcy = c ? (c.height - 44) / dpr2 : 0;
      const rotateCw = document.elementFromPoint(rcx, rcy);
      fsInfo = {
        display: getComputedStyle(fs).display,
        opacity: getComputedStyle(fs).opacity,
        rect: { w: Math.round(fr.width), h: Math.round(fr.height), right: Math.round(window.innerWidth - fr.right), bottom: Math.round(window.innerHeight - fr.bottom) },
        selfHit: btnCenter ? (btnCenter.id || btnCenter.tagName) : 'none',
        rotateCwHit: rotateCw ? (rotateCw.id || rotateCw.tagName) : 'none',
        // The browser Fullscreen API's state (the task's honest limit
        // made observable: a CDP click MAY grant the transient user
        // activation — the element/enabled flags are the verdict;
        // the opacity is the html:fullscreen CSS flip's observable
        // (the button's IN-fullscreen style), the display is the gate's).
        fullscreen: { element: !!document.fullscreenElement, enabled: document.fullscreenEnabled },
      };
    }
    return JSON.stringify({
      state: (window.ohStatus ? (window.ohStatus().match(/state=([^;\\s]+)/) || [])[1] : null) || null,
      viewport: { w: window.innerWidth, h: window.innerHeight, dpr: window.devicePixelRatio },
      active: a ? (a.tagName + (a.id ? ':' + a.id : '')) : 'none',
      ime: !!ime,
      imeValue: ime ? ime.value : null,
      canvas: c && cr ? { backing: c.width, css: Math.round(cr.width), dpr: window.devicePixelRatio, ok: Math.abs(c.width - cr.width * window.devicePixelRatio) <= 2 } : null,
      fs: fsInfo,
      // issue 057 — the focus counters (the reproduction's
      // instrumentation, the installer block above) + the live
      // visibility / fullscreen flags (a held input must see neither
      // flip while it is held).
      focus: Object.assign({
        hidden: document.visibilityState,
        fullscreen: !!document.fullscreenElement,
      }, window.__fc || {}),
      dom: keylog,
    });
  })()`, returnByValue: true });
  const v = r.result?.result?.value ?? '(step check failed)';
  console.log(`  [step] ${label} -> ${v}`);
  let dom = null;
  try { dom = JSON.parse(v).dom; } catch { /* the check itself failed */ }
  if (dom && dom.length) { console.log(`  [dom] ${label}: ${dom.join(' | ')}`); }
};

// issue 041 — the web's audio needs a user gesture to unlock the Web Audio
// context (emscripten auto-resumes on the first key/pointer input). Whether the
// HEADLESS CDP input actually grants user activation is exactly what decides
// if the AudioContext can run (and thus if a headless probe can prove
// audibility). `document.hasUserActivation()` is the browser's own answer; we
// sample it right after each keypress (transient activation lasts ~5s, so we
// check before the post-press sleep lets it expire).
let userActivation = false;
const checkActivation = async () => {
  try {
    const r = await send('Runtime.evaluate', { expression: `document.hasUserActivation ? document.hasUserActivation() : null`, returnByValue: true });
    if (r.result?.result?.value === true) userActivation = true;
  } catch { /* the evaluate failed — leave the flag as-is */ }
};

const perKey = Math.max(4, Math.floor(secs / (Math.max(keys.length, seq.length) + 1)));
await sleep(perKey * 1000);
// issue 054 — the boot gate: the first input must land AFTER the
// composition boots (the pre-boot keypresses reach no listener — the
// `attach_web_listeners` installs on window creation, the wasm's own
// boot), so wait for the port's own `booted=true` status line before the
// seq (capped: a boot that never reports fails the probe's exit code
// anyway — the wait just stops guessing the perKey boot margin).
if (phone) {
  // The viewport convergence (the phone block's note above): measure
  // the shell's ACTUAL layout viewport (post-load) and pin the override
  // to it (the fixed point — the canvas's 100vw then fills the viewport
  // exactly; the screenshot's device px == the game view's pixels).
  for (let i = 0; i < 3; i++) {
    const r = await send('Runtime.evaluate', { expression: 'JSON.stringify({ w: window.innerWidth, h: window.innerHeight })', returnByValue: true });
    const m = JSON.parse(r.result?.result?.value ?? 'null');
    if (!m || !m.w || !m.h) break;
    if (m.w === 390 && m.h === 844) break; // the override took verbatim
    await send('Emulation.setDeviceMetricsOverride', { width: m.w, height: m.h, deviceScaleFactor: 3, mobile: true });
    await sleep(300);
  }
  const converged = await send('Runtime.evaluate', { expression: 'JSON.stringify({ w: window.innerWidth, h: window.innerHeight, dpr: window.devicePixelRatio })', returnByValue: true });
  console.log(`  … phone viewport converged (${converged.result?.result?.value ?? '?'})`);
}
if (seq.length > 0) {
  for (let i = 0; i < 120; i++) {
    const st = await stateOf();
    if (st && /booted=true/.test(st)) break;
    await sleep(1000);
  }
  const bootedStatus = await stateOf();
  console.log(`  … boot settled (${(bootedStatus ?? '(no status)').slice(0, 120)})`);
}
if (seq.length > 0) {
  // issue 046 — the ACTION script (the docs above): the per-step
  // keypresses / the `Input.insertText` IME-style commits / the
  // `Runtime.evaluate` synthetic-event dispatches, each followed by the
  // page's own observables (the state / the focus / the `#ime` value /
  // the canvas's real-size invariant).
  for (const item of seq) {
    if (item.startsWith('i:')) {
      console.log(`  … insertText ${JSON.stringify(item.slice(2))}`);
      await send('Input.insertText', { text: item.slice(2) });
    } else if (item.startsWith('t:')) {
      // issue 054 — the HELD touch (the docs above): CDP
      // `Input.dispatchTouchEvent`'s touchStart → DUR ms → touchEnd.
      // The X,Y are the PORT'S view-pixel space (the `button_centers`
      // / the grid's thirds' space — the `canvas_position`'s ×
      // device-pixel-ratio space): the probe reads the canvas's real
      // size + the DPR and divides to the CDP's CSS-px coordinates
      // (the headless viewport's size is NOT the emulated one — the
      // 054 run's measurement: the 390×844 @3 override's viewport
      // comes out 980×2121 CSS @3 in this Chrome; the port's view-
      // px space is the invariant half). The `;` is the internal
      // separator (the seq's `,` would split the coordinate triple).
      // Named presets (the 054 evidence's vocabulary — the grid's
      // zones' centers in the C++'s thirds' math + the MENU corner):
      // `menu` (the `Exit` center, the `button_centers`'s (44, 44)),
      // `ok` ((w/2, h/2) — the middle band's middle), `left`
      // ((w/6, h/2)), `right` ((5w/6, h/2)), `up` ((w/2, h/6)),
      // `down` ((w/2, 5h/6)).
      await ensureCmListener();
      const tparts = item.slice(2).split(';');
      const tname = tparts[0];
      const tnums = tparts.map(Number);
      let vx, vy, dur;
      if (Number.isNaN(tnums[0])) {
        const r = await send('Runtime.evaluate', { expression: '(() => { const c = document.getElementById("c"); return JSON.stringify({ w: c.width, h: c.height }); })()', returnByValue: true });
        const g = JSON.parse(r.result?.result?.value);
        const P = {
          menu: [44, 44],
          ok: [g.w / 2, g.h / 2],
          left: [g.w / 6, g.h / 2],
          right: [5 * g.w / 6, g.h / 2],
          up: [g.w / 2, g.h / 6],
          down: [g.w / 2, (5 * g.h) / 6],
        };
        if (!P[tname]) throw new Error(`web-probe: unknown touch preset ${item}`);
        [vx, vy] = P[tname];
        dur = tnums[1];
      } else {
        [vx, vy, dur] = tnums;
      }
      const dprR = await send('Runtime.evaluate', { expression: 'window.devicePixelRatio', returnByValue: true });
      const dpr = dprR.result?.result?.value || 1;
      const tx = vx / dpr; const ty = vy / dpr;
      console.log(`  … touch hold (view ${vx}, ${vy} = css ${Math.round(tx)}, ${Math.round(ty)}) for ${dur} ms`);
      await send('Input.dispatchTouchEvent', {
        type: 'touchStart',
        touchPoints: [{ x: tx, y: ty, id: 1, radiusX: 2, radiusY: 2, force: 1 }],
      });
      await sleep(dur);
      await send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
      await checkActivation();
    } else if (item.startsWith('h:')) {
      // issue 057 — the HELD key (the held-input reproduction): a CDP
      // `Input.dispatchKeyEvent` keyDown, held for MS ms, then the
      // keyUp — the browser sends NO further keydown for a held key
      // (no OS repeats on a synthetic key, and a real held key's OS
      // repeats are the same test: the port must hold the level
      // between them). The observable is the game's own: the
      // tutorial level's tracked "rotated counter-clockwise" verdict
      // (the pre-fix port's held level was reverted to its pre-frame
      // value every frame — the pre-fold restore — so the game saw
      // the press for the fold's frame only; the fixed port holds the
      // level until the release's event, the C++'s persistent
      // inputState). An optional third field (`h:KEY;MS;MIDNAME`) captures a
      // screenshot MID-HOLD (at 40% of MS) so the ship's silhouette
      // change DURING the hold is measurable (the change-detection
      // observable — robust where the absolute angle is noisy).
      const hparts = item.slice(2).split(';');
      const hkey = hparts[0];
      const hdur = Number(hparts[1] || 500);
      const hmid = hparts[2] || null;
      // The DOM's `KeyboardEvent.code` for the arrow keys is the
      // `Arrow*` word (the port's `web_key_to_sfml` maps on `code`:
      // "ArrowLeft" → Key::LEFT — a bare "Left" code maps to nothing),
      // and the `key` is the same `Arrow*` word (a bare "Left"
      // key+code arrives with BOTH empty — the first pre-fix run's
      // `down://`; only a letter's `key` is the bare char).
      const ARROW_CODE = { Left: 'ArrowLeft', Right: 'ArrowRight', Up: 'ArrowUp', Down: 'ArrowDown' };
      const hcode = hkey.length === 1 ? `Key${hkey.toUpperCase()}` : (ARROW_CODE[hkey] ?? hkey);
      const hdomKey = hkey.length === 1 ? hkey : (ARROW_CODE[hkey] ?? hkey);
      const hvk = VK[hdomKey] ?? (hkey.length === 1 ? hkey.toUpperCase().charCodeAt(0) : 0);
      console.log(`  … holding ${hkey} for ${hdur} ms`);
      await send('Input.dispatchKeyEvent', { type: 'rawKeyDown', key: hdomKey, code: hcode, windowsVirtualKeyCode: hvk, nativeVirtualKeyCode: hvk, text: hkey.length === 1 ? hkey : undefined });
      if (hmid) {
        await sleep(Math.round(hdur * 0.4));
        await snap(hmid);
        await sleep(hdur - Math.round(hdur * 0.4));
      } else {
        await sleep(hdur);
      }
      await send('Input.dispatchKeyEvent', { type: 'keyUp', key: hdomKey, code: hcode, windowsVirtualKeyCode: hvk, nativeVirtualKeyCode: hvk });
      await checkActivation();
    } else if (item.startsWith('m:')) {
      // issue 057 — the HELD mouse button (the held-input
      // reproduction's mouse half): a CDP `Input.dispatchMouseEvent`
      // mousePressed at the canvas's center (the browser synthesizes
      // the canvas `pointerdown` the port's pointer feed rides on),
      // held for MS ms, then the mouseReleased. Same observable as
      // the held key (the game's tracked rotation verdict — the
      // `h:`'s note); LButton is the rotate-CCW default bind (the
      // config's `t_rotate_ccw`). An optional third field
      // (`m:BTN;MS;MIDNAME`) captures a screenshot MID-HOLD (at 40%
      // of MS).
      const mparts = item.slice(2).split(';');
      const mbtn = mparts[0] === 'right' ? 'right' : 'left';
      const mdur = Number(mparts[1] || 500);
      const mmid = mparts[2] || null;
      const mp = await send('Runtime.evaluate', { expression: 'JSON.stringify({ x: window.innerWidth / 2, y: window.innerHeight / 2 })', returnByValue: true });
      const g = JSON.parse(mp.result?.result?.value ?? '{}');
      console.log(`  … holding mouse ${mbtn} at ${g.x}, ${g.y} for ${mdur} ms`);
      await send('Input.dispatchMouseEvent', { type: 'mousePressed', x: g.x, y: g.y, button: mbtn, buttons: mbtn === 'left' ? 1 : 2, clickCount: 1 });
      if (mmid) {
        await sleep(Math.round(mdur * 0.4));
        await snap(mmid);
        await sleep(mdur - Math.round(mdur * 0.4));
      } else {
        await sleep(mdur);
      }
      await send('Input.dispatchMouseEvent', { type: 'mouseReleased', x: g.x, y: g.y, button: mbtn, buttons: 0, clickCount: 1 });
      await checkActivation();
    } else if (item.startsWith('mv:')) {
      // issue 058 (29m) — the COORDINATE mouse MOVE (the click
      // acceptance's positioning half): a CDP
      // `Input.dispatchMouseEvent` mouseMoved at X;Y (the CSS viewport's
      // coords — the `m:`'s canvas-center's coordinate space; the
      // browser delivers the canvas `pointermove` the port's feed rides
      // on). The port folds it into the `MouseMoved` event (the
      // `mouse_pixel`'s update — the menu's hover hit-test's source, the
      // C++'s `getMousePosition`'s equivalent) + the 29u1 cursor
      // re-show (a REAL move — the `actuallyMoved`'s delta). No button
      // is pressed (the `m:`'s press's complement: position without
      // edge).
      const mparts = item.slice(3).split(';');
      const mx = Number(mparts[0]);
      const my = Number(mparts[1]);
      console.log(`  … mouse move to ${mx}, ${my}`);
      await send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: mx, y: my });
      await checkActivation();
    } else if (item.startsWith('mc:')) {
      // issue 058 (29m) — the COORDINATE mouse CLICK (the menu item's
      // click-to-activate's measurement — the plan's item 8, "menu
      // click -> item exec"): a CDP mousePressed at X;Y held for MS ms
      // (default 300 — the orchestrator's `m:left;60`'s no-op is the
      // 60 ms' headless-frame artifact: at the SwiftShader pace (~20
      // fps) a 60 ms hold spans < 2 frames, and the port's DRAW-TIME
      // level sample (the C++'s `isButtonPressed`'s equivalent — the
      // C++ would miss a sub-frame press the same way) needs the
      // press visible for >= 1 full frame after a NOT-pressed draw),
      // then the mouseReleased. NO implicit move: the port's
      // `pointerdown` fold emits the `MouseMoved` (the press's
      // position) BEFORE the `MouseButtonPressed` (window.rs's
      // `pointer_down`'s mouse arm) — a click whose position arrives
      // with the press is well-defined (the C++'s SFML live-position
      // read's equivalent), so the no-prior-move case is this step's
      // own territory (the (a)-vs-(b)'s distinguisher's half); the
      // move-first case is the `mv:` + `mc:` pair.
      const mparts = item.slice(3).split(';');
      const mx = Number(mparts[0]);
      const my = Number(mparts[1]);
      const mdur = Number(mparts[2] || 300);
      console.log(`  … clicking mouse at ${mx}, ${my} (held ${mdur} ms, no prior move)`);
      await send('Input.dispatchMouseEvent', { type: 'mousePressed', x: mx, y: my, button: 'left', buttons: 1, clickCount: 1 });
      await sleep(mdur);
      await send('Input.dispatchMouseEvent', { type: 'mouseReleased', x: mx, y: my, button: 'left', buttons: 0, clickCount: 1 });
      await checkActivation();
    } else if (item === 'cm' || item.startsWith('cm:')) {
      // issue 058 (F37) — the RIGHT-CLICK (the context menu's trigger):
      // a CDP `Input.dispatchMouseEvent` right press + release — the
      // browser dispatches the page's `contextmenu` event (the shell's
      // F37 listener's `defaultPrevented` is the step check's dom log's
      // `cm:<target>/prev:<bool>` entry; the native menu itself does
      // not render headlessly — the default-action suppression is the
      // measurement). `cm` = the canvas's center (the game page),
      // `cm:ime` = the #ime input's center (the exclusion's proof: its
      // contextmenu must NOT be prevented).
      await ensureCmListener();
      if (item === 'cm:ime') {
        // The #ime's exclusion, measured on the element ITSELF: the
        // input sits off-viewport (the soft-keyboard field's CSS — a
        // CDP click at its center would land on the <html>), so the
        // synthetic dispatch (the browser's own event constructor,
        // through the page's own listener chain) is the honest
        // measurement of the exclusion's logic (its prev must be
        // false — the desktop's paste / copy affordance stays).
        const r = await send('Runtime.evaluate', { expression: `(() => {
          const ime = document.getElementById('ime');
          if (!ime) return 'no #ime on the page';
          const ev = new MouseEvent('contextmenu', { bubbles: true, cancelable: true, button: 2 });
          ime.dispatchEvent(ev);
          return 'synthetic #ime contextmenu: prevented=' + ev.defaultPrevented + ' target=' + ev.target.id;
        })()`, returnByValue: true });
        console.log(`  … ${r.result?.result?.value ?? '(evaluate failed)'}`);
      } else {
        const pr = await send('Runtime.evaluate', { expression: `(() => {
          const c = document.getElementById('c');
          if (!c) return null;
          const r = c.getBoundingClientRect();
          return JSON.stringify({ x: r.left + r.width / 2, y: r.top + r.height / 2 });
        })()`, returnByValue: true });
        const p = JSON.parse(pr.result?.result?.value ?? 'null');
        if (!p) throw new Error('web-probe: the canvas is not on the page for the right-click');
        console.log(`  … right-click (contextmenu) at ${Math.round(p.x)}, ${Math.round(p.y)}`);
        await send('Input.dispatchMouseEvent', { type: 'mousePressed', x: p.x, y: p.y, button: 'right', buttons: 2, clickCount: 1 });
        await send('Input.dispatchMouseEvent', { type: 'mouseReleased', x: p.x, y: p.y, button: 'right', buttons: 0, clickCount: 1 });
        await checkActivation();
      }
    } else if (item.startsWith('hc:')) {
      // issue 058 (F37) — the HELD-INPUT focus-loss probe (the
      // operator's "held input dies"'s second suspect): a CDP keyDown
      // held for MS, a RIGHT-CLICK at the 50% mark (the context menu's
      // appearance's focus steal — if it takes the window's focus the
      // 032 release-on-blur fold fires LostFocus and releases every
      // held input; the focus counters' blur half + the port's state
      // are the observables), then the keyUp. The post-step report's
      // `focus:` counters say whether the right-click ever blurred the
      // window (a headless context menu that does NOT take focus is
      // the honest negative: the probe says so, and the suppression's
      // defaultPrevented half is the measurement either way).
      await ensureCmListener();
      const hparts = item.slice(3).split(';');
      const hkey = hparts[0];
      const hdur = Number(hparts[1] || 2000);
      const hcode = hkey.length === 1 ? `Key${hkey.toUpperCase()}` : hkey;
      const hvk = VK[hkey] ?? (hkey.length === 1 ? hkey.toUpperCase().charCodeAt(0) : 0);
      console.log(`  … holding ${hkey} for ${hdur} ms, right-click at the 50% mark`);
      await send('Input.dispatchKeyEvent', { type: 'rawKeyDown', key: hkey, code: hcode, windowsVirtualKeyCode: hvk, nativeVirtualKeyCode: hvk, text: hkey.length === 1 ? hkey : undefined });
      await sleep(Math.round(hdur / 2));
      const pr = await send('Runtime.evaluate', { expression: `JSON.stringify({ x: window.innerWidth / 2, y: window.innerHeight / 2 })`, returnByValue: true });
      const p = JSON.parse(pr.result?.result?.value ?? 'null');
      await send('Input.dispatchMouseEvent', { type: 'mousePressed', x: p.x, y: p.y, button: 'right', buttons: 2, clickCount: 1 });
      await send('Input.dispatchMouseEvent', { type: 'mouseReleased', x: p.x, y: p.y, button: 'right', buttons: 0, clickCount: 1 });
      await sleep(Math.round(hdur / 2));
      await send('Input.dispatchKeyEvent', { type: 'keyUp', key: hkey, code: hcode, windowsVirtualKeyCode: hvk, nativeVirtualKeyCode: hvk });
      await checkActivation();
    } else if (item.startsWith('s:')) {
      // issue 054 — the per-step screenshot (the docs above): the
      // evidence's stills (the operator-visible halves of the items),
      // `--shots/NAME.png`.
      await snap(item.slice(2));
    } else if (item.startsWith('e64:')) {
      // A base64-encoded `Runtime.evaluate` expression (the comma-safe
      // form — a JS body with commas would otherwise split on the
      // seq's `,` delimiter): the synthetic composition / backspace
      // event dispatches (the issue 046's four-cases' DOM half).
      const expr = Buffer.from(item.slice(4), 'base64').toString('utf8');
      console.log(`  … evaluate64 ${JSON.stringify(expr.slice(0, 80))}${expr.length > 80 ? '…' : ''}`);
      await send('Runtime.evaluate', { expression: expr, returnByValue: true });
    } else if (item.startsWith('e:')) {
      console.log(`  … evaluate ${JSON.stringify(item.slice(2).slice(0, 80))}${item.slice(2).length > 80 ? '…' : ''}`);
      await send('Runtime.evaluate', { expression: item.slice(2), returnByValue: true });
    } else {
      console.log(`  … pressing ${item}`);
      await press(item);
      await checkActivation();
    }
    await sleep(perKey * 1000);
    await stepCheck(item);
  }
} else {
  for (const k of keys) { console.log(`  … pressing ${k}`); await press(k); await checkActivation(); await sleep(perKey * 1000); }
}
await checkActivation();
await stepCheck('final');
await sleep(2000);

// The state timeline comes from the log lines the port emits: status booted=…;state=…
const states = [];
for (const l of logs) { const m = /state=([^\s]+)/.exec(l); if (m && states[states.length - 1] !== m[1]) states.push(m[1]); }
const boots = logs.filter((l) => /status booted=true/.test(l));
const lastStatus = [...logs].reverse().find((l) => /status booted=/.test(l)) || '(no status line seen)';
const shot = await send('Page.captureScreenshot', { format: 'png' });
if (shot.result?.data) writeFileSync(shotPath, Buffer.from(shot.result.data, 'base64'));

console.log(`\nweb-probe: ${url}`);
console.log(`  booted        : ${boots.length ? 'yes' : 'NO'}  (${boots.length} status lines)`);
console.log(`  last status   : ${lastStatus.slice(0, 120)}`);
console.log(`  state timeline: ${states.length ? states.join(' -> ') : '(none)'}`);
// issue 041 — surface the port's own [oh-audio] diagnostics (which world the
// audio device is in + the unlock/first-play observables). These arrive as
// console.log (the Rust stdio path is swallowed on the web), so they are in
// `logs`, not the derived `state=` timeline above.
const audioDiag = logs.filter((l) => /\[oh-audio\]/.test(l));
console.log(`  audio diagnostics:`);
if (audioDiag.length) { for (const l of audioDiag) console.log(`    ${l}`); }
else { console.log(`    (no [oh-audio] lines — the audio device did not log)`); }
// issue 046 (F33) — the fullscreen button's wiring evidence (the
// `[oh-web-fs]` line — the click sink's config toggle + the browser
// Fullscreen API's verdict: a `requestFullscreen` needs a transient
// user activation a CDP click cannot grant (the 041 audio twin), so a
// headless run's EXPECTED outcome is the `rejected:` line — the
// wiring's proof, the transition's the operator's real tap).
const fsDiag = logs.filter((l) => /\[oh-web-fs\]/.test(l));
console.log(`  fullscreen wiring:`);
if (fsDiag.length) { for (const l of fsDiag) console.log(`    ${l}`); }
else { console.log(`    (no [oh-web-fs] line — the #fs click was not exercised in this run)`); }
console.log(`  user activation : ${userActivation
    ? 'yes — the headless input granted user activation (the AudioContext can run; audibility is real, not just scheduled)'
    : 'no — the headless input did NOT grant user activation (the AudioContext stays suspended; a real user gesture would resume it — headless cannot prove audibility)'}`);
// issue 057 — the focus / blur / visibility / fullscreen counters
// (the held-input reproduction's instrumentation — a spurious blur or
// a hidden-tab flip mid-hold would have driven the port's `LostFocus`
// fold and cleared the held levels; the counts say whether that
// happened).
{
  const r = await send('Runtime.evaluate', {
    expression: 'JSON.stringify(Object.assign({ hidden: document.visibilityState, fullscreen: !!document.fullscreenElement }, window.__fc || {}))',
    returnByValue: true,
  });
  console.log(`  focus counters: ${r.result?.result?.value ?? '(unreadable)'}`);
}
// issue 058 (F37) — the context-menu verdict (the F37 suppression's
// measurement: the browser's own `defaultPrevented` on the page's
// contextmenu events — the canvas / document's PREVENTED (no menu
// appears), the #ime's NOT (the exclusion). With `--expect-cm` the
// verdict is a gate: a canvas contextmenu must have been prevented
// and no #ime one (a missing canvas event — the right-click not
// reaching the page — is a FAIL, not a pass: the probe measured
// nothing). The long-press's contextmenu (the mobile half) arrives
// through the same log (the `t:` held touch's browser synthesis).
const cmEntries = [];
{
  // The __cmlog (the undrained array — the step checks drain the
  // __keylog, so the verdict's source is the dedicated one).
  const r = await send('Runtime.evaluate', { expression: 'JSON.stringify(window.__cmlog || [])', returnByValue: true });
  try { cmEntries.push(...JSON.parse(r.result?.result?.value ?? '[]')); } catch { /* the read failed — nothing measured */ }
}
let cmOk = true;
if (argv.includes('--expect-cm')) {
  const canvasPrevented = cmEntries.some((l) => !l.startsWith('cm:ime') && l.endsWith('/prev:true'));
  const imePrevented = cmEntries.some((l) => l.startsWith('cm:ime') && l.endsWith('/prev:true'));
  cmOk = canvasPrevented && !imePrevented;
}
console.log(`  context menu  : ${cmEntries.length ? cmEntries.join(' | ') : '(no contextmenu event reached the page)'}`);
if (argv.includes('--expect-cm')) {
  console.log(`  (gate)          : ${cmOk
    ? 'PASS — the canvas / document contextmenu was prevented (no menu appears) and the #ime\'s was not (the exclusion)'
    : 'FAIL — ' + (cmEntries.length ? 'the prevention shape is wrong (want: canvas prev:true, ime prev:false)' : 'no contextmenu event was measured (the right-click did not reach the page\'s listener)')}`);
}
console.log(`  probe         : ${await stateOf() ?? '(Module.status not reachable via evaluate)'}`);
console.log(`  screenshot    : ${shotPath}`);
console.log(`  non-200       : ${bad.length ? bad.join(' | ') : 'none'}`);
console.log(`  failed loads  : ${failed.length ? failed.join(' | ') : 'none'}`);
console.log(`  console errors: ${errors.length ? errors.map((e) => e.slice(0, 160)).join(' | ') : 'none'}`);
chrome.kill('SIGKILL');

const realErrors = errors.filter((e) => !/favicon/i.test(e));
const realBad = bad.filter((b) => !/favicon/.test(b));
process.exit(boots.length > 0 && realErrors.length === 0 && realBad.length === 0 && (!argv.includes('--expect-cm') || cmOk) ? 0 : 1);
