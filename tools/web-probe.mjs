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
import { writeFileSync, existsSync } from 'node:fs';

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
//   keyboard's composition flows).
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

const CHROME = process.env.OH_CHROME
  || ['/home/gary/.agent-browser/browsers/chrome-148.0.7778.97/chrome',
      '/usr/bin/google-chrome', '/usr/bin/chromium'].find((p) => existsSync(p));
if (!CHROME) { console.error('web-probe: no chrome found; set OH_CHROME=/path/to/chrome'); process.exit(1); }
const PORT = Number(process.env.OH_CDP_PORT || 9335);

const chrome = spawn(CHROME, ['--headless=new', `--remote-debugging-port=${PORT}`, '--no-sandbox',
  '--disable-dev-shm-usage', '--use-angle=swiftshader', '--enable-unsafe-swiftshader',
  '--window-size=1280,720', '--no-first-run', `--user-data-dir=/tmp/oh-web-probe-profile-${process.pid}`, 'about:blank'],
  { stdio: ['ignore', 'ignore', 'ignore'] });

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
  await send('Emulation.setDeviceMetricsOverride', {
    width: 390, height: 844, deviceScaleFactor: 3, mobile: true,
  });
  console.log('  … phone viewport (390x844 @3, touch)');
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
    // issue 046 (F33) — the port's menu<->game transition EDGE (the
    // ohstatechange DOM event — the house pattern's edge, not a
    // poll): the #fs visibility gate's signal (its display flipping
    // between steps is the gate's observable proof).
    document.addEventListener('ohstatechange', () => { rec('ohstatechange', ''); });
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
const VK = { Enter: 13, Backspace: 8, Escape: 27 };
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
      active: a ? (a.tagName + (a.id ? ':' + a.id : '')) : 'none',
      ime: !!ime,
      imeValue: ime ? ime.value : null,
      canvas: c && cr ? { backing: c.width, css: Math.round(cr.width), dpr: window.devicePixelRatio, ok: Math.abs(c.width - cr.width * window.devicePixelRatio) <= 2 } : null,
      fs: fsInfo,
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
console.log(`  probe         : ${await stateOf() ?? '(Module.status not reachable via evaluate)'}`);
console.log(`  screenshot    : ${shotPath}`);
console.log(`  non-200       : ${bad.length ? bad.join(' | ') : 'none'}`);
console.log(`  failed loads  : ${failed.length ? failed.join(' | ') : 'none'}`);
console.log(`  console errors: ${errors.length ? errors.map((e) => e.slice(0, 160)).join(' | ') : 'none'}`);
chrome.kill('SIGKILL');

const realErrors = errors.filter((e) => !/favicon/i.test(e));
const realBad = bad.filter((b) => !/favicon/.test(b));
process.exit(boots.length > 0 && realErrors.length === 0 && realBad.length === 0 ? 0 : 1);
