#!/usr/bin/env node
// tools/web-fs-probe.mjs — the issue 055 probe: the web fullscreen button +
// the canvas real-size invariant across a viewport change, driven over CDP.
//
// WHY THIS EXISTS (the operator's report, verbatim, on Android Chrome):
//   "the full screen button needs to be on top right and stay present.
//    sometimes the browser itself gets out of fullscreen and i can no
//    longer reach the button. also when that happens the scaling gets
//    out of sync and never recovers."
// The "out of sync and never recovers" half is a bug at the window layer:
// the canvas backing store must track its CSS box × the devicePixelRatio
// (the 025g invariant, issue 32's real-size path), and a viewport change
// (the fullscreen exit's resize) that the page's `resize` listener
// measures BEFORE the layout / the DPR settle leaves the backing store
// stale with nothing to correct it. The probe's measured fact that
// shapes the fix: a DPR-ONLY viewport change (the Android
// `devicePixelRatio` change across a fullscreen transition — the task's
// second suspect) fires NO `resize` event and NO `matchMedia` change at
// all — the CDP `Emulation.setDeviceMetricsOverride` simulates that
// change headlessly (a real fullscreen transition needs a transient user
// activation headless cannot grant — the task's honest limit, the 046/041
// twin).
//
// THREE MODES:
//   --scale (default) — the BEFORE/AFTER pair: boot at the phone viewport
//     (390×844 @3, the operator's Android Chrome shape), then drive the
//     viewport through the two suspects — a SIZE change (the fullscreen
//     exit's viewport change; the `resize` event's path) and a DPR-ONLY
//     change (the no-event dimension) — sampling the invariant (canvas
//     backing store == CSS box × DPR) at 20 / 150 / 1200 ms after each
//     trigger. The invariant at the SETTLED sample (1200 ms) is the
//     acceptance: a pre-fix build leaves it violated there (nothing
//     re-fires the `resize` — the "never recovers"), a fixed build
//     restores it within a frame of the settle.
//   --hit              — the issue 055 item-1 hit-test proof: NOTHING THE
//     GAME OWNS IS SWALLOWED, in the menu state AND the game state:
//     `document.elementFromPoint` at (a) the #fs button's own center
//     (must be ITSELF — reachable, present), (b) the gameplay overlay's
//     RotateCW corner (046's property — the button left that corner),
//     and in the game state (c) the F31 action row's three buttons
//     (REPLAY / RESTART / PAUSE — the port's own top-right geometry,
//     `touch_overlay.rs`'s `button_centers`'s top row: PAD 12 +
//     BUTTON_HALF 32 — must each hit-test to the CANVAS: the button's
//     below-the-band offset is the collision resolution), plus the
//     button's geometry assertions (display block in the game state —
//     the operator's "stay present"; top below the band's 76-real-px
//     bottom edge — the no-swallow proof).
//   --wiring           — the issue 055 item-2 desync proof: the synthetic
//     `#fs.click()` (the 046's wiring evidence — the `[oh-web-fs]
//     fullscreen -> true (browser: requestFullscreen() called …)` line),
//     then — WHEN the headless CDP grants the transient user activation
//     (046's observed run-dependent grant) — a REAL CDP `Escape` keypress
//     (the EXTERNAL exit the game did not ask for — the operator's exact
//     "the browser itself gets out of fullscreen") and the
//     `[oh-web-fs] fullscreenchange -> false (config true -> false)` line
//     (the config's re-sync from `document.fullscreenElement` — the
//     desync's close: the next tap's direction is the browser's ACTUAL
//     state, the config cannot outlive the browser's exit). When CDP
//     denies the activation (the `promise rejected` line), the probe says
//     so plainly — the transition's real grant is the operator's tap.
//
//   node tools/web-fs-probe.mjs http://127.0.0.1:8067/ --scale
//   node tools/web-fs-probe.mjs http://127.0.0.1:8067/ --scale --hit --wiring
//
// EXIT: 0 when every SETTLED invariant sample holds AND (with --hit)
// every hit-test assertion passes AND (with --wiring, when the CDP
// grants the activation) the re-sync line landed; 1 otherwise. The
// probe prints the event-time vs settled readings (the page-side
// `__reslog`'s mirror of the Rust listener's view) — the diagnostic
// that separates "the listener fired before the layout / the DPR
// settled" from "the size was right and something else is wrong".

import { spawn } from 'node:child_process';
import { setTimeout as sleep } from 'node:timers/promises';
import { existsSync } from 'node:fs';

const argv = process.argv.slice(2);
const url = argv.find((a) => !a.startsWith('--')) || 'http://127.0.0.1:8067/';
const flags = argv.filter((a) => a.startsWith('--'));
const scale = flags.includes('--scale') || flags.length === 0;
const hit = flags.includes('--hit');
const wiring = flags.includes('--wiring');

const CHROME =
  process.env.OH_CHROME ||
  ['/home/gary/.agent-browser/browsers/chrome-148.0.7778.97/chrome', '/usr/bin/google-chrome', '/usr/bin/chromium'].find((p) => existsSync(p));
if (!CHROME) { console.error('web-fs-probe: no chrome found; set OH_CHROME=/path/to/chrome'); process.exit(1); }
// The port is per-PID (like the user-data-dir): a stale Chrome from a
// crashed run (a pre-fix probe left one on the shared port — the next
// run silently connected to it, the stale-browser's persisted profile
// confusing the boot flow) cannot be reached on this run's port.
const PORT = Number(process.env.OH_CDP_PORT) || (9300 + (process.pid % 90));

// The operator's shape (the web-probe's --phone): 390×844 @3.
const BASE = { width: 390, height: 844, deviceScaleFactor: 3, mobile: true };

const chrome = spawn(CHROME, ['--headless=new', `--remote-debugging-port=${PORT}`, '--no-sandbox',
  '--disable-dev-shm-usage', '--use-angle=swiftshader', '--enable-unsafe-swiftshader',
  '--window-size=1280,720', '--no-first-run', `--user-data-dir=/tmp/oh-web-fs-probe-profile-${process.pid}`, 'about:blank'],
  { stdio: ['ignore', 'ignore', 'ignore'] });
// The cleanup guarantee (the probe crashed once leaving its Chrome alive on
// the shared CDP port — the next run then connected to the STALE browser:
// the `exit` handler fires on normal exit, uncaught exceptions and
// process.exit alike, so a crashed probe never wedges the port):
process.on('exit', () => { try { chrome.kill('SIGKILL'); } catch { /* already dead */ } });

const http = async (p, i) => (await fetch(`http://127.0.0.1:${PORT}${p}`, i)).json();
let up = false;
for (let i = 0; i < 60 && !up; i++) { try { await http('/json/version'); up = true; } catch { await sleep(500); } }
if (!up) { chrome.kill('SIGKILL'); console.error('web-fs-probe: chrome never opened its debug port'); process.exit(1); }

const target = await http(`/json/new?${encodeURIComponent(url)}`, { method: 'PUT' });
const ws = new WebSocket(target.webSocketDebuggerUrl);
let id = 0; const pending = new Map();
const errors = []; const consoleLogs = [];
ws.addEventListener('message', (ev) => {
  const m = JSON.parse(ev.data);
  if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); return; }
  if (m.method === 'Runtime.consoleAPICalled') {
    const text = (m.params?.args || []).map((a) => a.value ?? a.description ?? '').join(' ');
    consoleLogs.push(text);
  } else if (m.method === 'Runtime.exceptionThrown') {
    errors.push(`[exception] ${m.params?.exceptionDetails?.exception?.description || m.params?.exceptionDetails?.text}`);
  }
});
const send = (method, params = {}) => new Promise((r) => { const i = ++id; pending.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
await new Promise((r) => ws.addEventListener('open', r));
await send('Runtime.enable');
await send('Network.enable');
await send('Page.enable');
await send('Network.setCacheDisabled', { cacheDisabled: true });

// The baseline phone viewport BEFORE the navigation (the boot measures
// into it — the issue 32's real-size start).
await send('Emulation.setDeviceMetricsOverride', { ...BASE });
await send('Page.navigate', { url });

const ev = async (expression) => {
  const r = await send('Runtime.evaluate', { expression, returnByValue: true });
  return r.result?.result?.value ?? null;
};
const evExpr = (js) => ev(`(function(){ ${js} })()`);

// Wait for document ready + the boot's `status booted=true` line.
for (let i = 0; i < 120; i++) {
  const rs = await ev('document.readyState');
  if (rs === 'complete' || rs === 'interactive') break;
  await sleep(250);
}
let booted = false;
for (let i = 0; i < 120; i++) {
  const s = await ev(`typeof window.ohStatus === 'function' ? window.ohStatus() : ''`);
  if (s && /booted=true/.test(s)) { booted = true; break; }
  await sleep(500);
}
if (!booted) { console.error('web-fs-probe: the module never reported booted=true'); chrome.kill('SIGKILL'); process.exit(1); }
console.log('  booted (status booted=true)');

// The page-side RESIZE DIAGNOSTIC: what the browser's `resize` event
// carried at EVENT TIME (the same view the Rust listener reads — the
// probe's listener runs in the same task, after the port's). This is
// the tool that separates "the listener fired before the layout / the
// DPR settled" from "the size was right". Each entry: the inner
// viewport, the DPR, the canvas's CSS box (the forced-layout read) and
// the backing store, at the moment the event fired.
await evExpr(`
  window.__reslog = [];
  window.addEventListener('resize', () => {
    const c = document.getElementById('c');
    const r = c ? c.getBoundingClientRect() : null;
    window.__reslog.push({
      t: Math.round(performance.now()),
      inner: [window.innerWidth, window.innerHeight],
      dpr: window.devicePixelRatio,
      css: r ? [Math.round(r.width * 100) / 100, Math.round(r.height * 100) / 100] : null,
      backing: c ? [c.width, c.height] : null,
    });
    if (window.__reslog.length > 50) window.__reslog.shift();
  });
  return true;
`);

// The invariant sample (the 025g class): the canvas backing store vs
// its CSS box × the DPR (the port's `web_canvas_real_size` — the
// truncating `as u32`, so the expected is `Math.floor(css * dpr)`).
const sample = async (label) => {
  const v = await evExpr(`
    const c = document.getElementById('c');
    const r = c.getBoundingClientRect();
    const dpr = window.devicePixelRatio;
    const expected = [Math.floor(r.width * dpr), Math.floor(r.height * dpr)];
    return {
      label: ${JSON.stringify(label)},
      inner: [window.innerWidth, window.innerHeight],
      dpr,
      css: [r.width, r.height],
      backing: [c.width, c.height],
      expected,
      ok: c.width === expected[0] && c.height === expected[1],
      state: (window.ohStatus ? (window.ohStatus().match(/state=([^;\\s]+)/) || [])[1] : null),
    };
  `);
  const mark = v && v.ok ? 'OK ' : 'FAIL';
  console.log(`  [${mark}] ${label}: inner=${v.inner} dpr=${v.dpr} css=${v.css} backing=${v.backing} expected=${v.expected}${v.state ? ' state=' + v.state : ''}`);
  return v;
};

const reslogDrain = async () => {
  const entries = await evExpr(`const e = window.__reslog.splice(0); return e;`);
  for (const e of entries || []) {
    console.log(`  [reslog] t=${e.t} inner=${e.inner} dpr=${e.dpr} css=${e.css} backing=${e.backing}`);
  }
};

// The virtual-key press (the CDP `Input.dispatchKeyEvent`'s keyDown+keyUp
// — the REAL browser input path, the 046's `press`'s shape): the game-
// driving keys (the boot's Enters, the name's char) + the `Escape`
// (the EXTERNAL fullscreen exit — the operator's "the browser itself
// gets out of fullscreen": a real key the page's keydown listener maps
// to the SFML Escape, NOT the #fs click's own exit path).
const press = async (key) => {
  const vk = key === 'Enter' ? 13 : key === 'Escape' ? 27 : key.length === 1 ? key.toUpperCase().charCodeAt(0) : 0;
  const code = key.length === 1 ? `Key${key.toUpperCase()}` : key;
  for (const type of ['keyDown', 'keyUp']) {
    await send('Input.dispatchKeyEvent', { type, key, code, windowsVirtualKeyCode: vk, nativeVirtualKeyCode: vk, text: type === 'keyDown' && key.length === 1 ? key : key === 'Enter' ? '\r' : undefined });
  }
  if (key === 'Escape') return { before: null, now: null }; // no state expectation (the fullscreen exit, not a menu action)
  const before = (await ev(`typeof window.ohStatus === 'function' ? (window.ohStatus().match(/state=([^;\\s]+)/) || [])[1] : ''`)) || '';
  for (let i = 0; i < 40; i++) {
    await sleep(250);
    const now = (await ev(`typeof window.ohStatus === 'function' ? (window.ohStatus().match(/state=([^;\\s]+)/) || [])[1] : ''`)) || '';
    if (now !== before) return { before, now };
  }
  return { before, now: before };
};

// The hit-test (item 1's acceptance, the 046's `elementFromPoint`
// property): nothing the game owns is swallowed. The overlay's
// geometry is in REAL pixels (the canvas backing store's space —
// `button_centers`'s PAD 12 / BUTTON_HALF 32 / MIDDLE_GAP 80);
// `elementFromPoint` wants CSS px, so divide by the DPR.
const hitTest = async (label) => {
  const ht = await evExpr(`
    const c = document.getElementById('c');
    const dpr = window.devicePixelRatio || 1;
    const at = (x, y) => { const e = document.elementFromPoint(x, y); return e ? (e.id || e.tagName) : 'none'; };
    // The F31 action row's centers (real px, the port's own geometry):
    // REPLAY (w/2-40, 44) / RESTART (w/2+40, 44) / PAUSE (w-44, 44) —
    // the row's bottom edge is y=76 real px (PAD 12 + BUTTON_HALF 32).
    const replay = at(c.width / 2 / dpr - 40 / dpr, 44 / dpr);
    const restart = at(c.width / 2 / dpr + 40 / dpr, 44 / dpr);
    const pause = at(c.width / dpr - 44 / dpr, 44 / dpr);
    // The issue 046's property (the RotateCW corner — the button's OLD
    // home, bottom-right): it must hit-test to the canvas (the button
    // moved to the top-right).
    const rotateCw = at(c.width / dpr - 44 / dpr, c.height / dpr - 44 / dpr);
    // The button itself (top-right, present in EVERY state — the
    // operator's "stay present"): its own center hit-tests to ITSELF
    // (reachable) + its geometry vs the row band (the collision
    // resolution: the button's top is BELOW the row's bottom edge, 76
    // real px → 76/dpr CSS px).
    const fs = document.getElementById('fs');
    const fr = fs ? fs.getBoundingClientRect() : null;
    const fsSelf = fr ? at(fr.left + fr.width / 2, fr.top + fr.height / 2) : 'none';
    return {
      row: { replay, restart, pause },
      rotateCw,
      fs: fr ? {
        self: fsSelf,
        display: getComputedStyle(fs).display,
        rect: [Math.round(fr.left), Math.round(fr.top), Math.round(fr.width), Math.round(fr.height)],
        rowBottomCss: 76 / dpr,
        belowRow: fr.top >= 76 / dpr - 0.5,
        inTopRight: fr.left >= window.innerWidth - fr.width - 13 && fr.top < window.innerHeight / 3,
      } : null,
    };
  `);
  console.log(`  [hit] ${label}: ${JSON.stringify(ht)}`);
  let ok = true;
  for (const [name, v] of Object.entries(ht.row)) {
    if (v !== 'c') { ok = false; console.log(`  [hit][FAIL] ${label}: ${name} center swallowed by ${v} (must be the canvas 'c' — the game's own button)`); }
  }
  if (ht.rotateCw !== 'c') { ok = false; console.log(`  [hit][FAIL] ${label}: RotateCW corner swallowed by ${ht.rotateCw} (issue 046's property regressed — the button left that corner)`); }
  if (!ht.fs) { ok = false; console.log(`  [hit][FAIL] ${label}: the #fs button is absent`); }
  else {
    if (ht.fs.self !== 'fs') { ok = false; console.log(`  [hit][FAIL] ${label}: the #fs center hit-tests to ${ht.fs.self} (must be itself — the operator's "reach the button")`); }
    if (ht.fs.display !== 'block') { ok = false; console.log(`  [hit][FAIL] ${label}: the #fs display is ${ht.fs.display} (the operator's "stay present")`); }
    if (!ht.fs.belowRow) { ok = false; console.log(`  [hit][FAIL] ${label}: the #fs top (${ht.fs.rect[1]}px) is NOT below the F31 row's band (76/dpr=${ht.fs.rowBottomCss.toFixed(1)}px) — it would swallow the PAUSE button's touches (046's avoided failure)`); }
    if (!ht.fs.inTopRight) { ok = false; console.log(`  [hit][FAIL] ${label}: the #fs is not in the top-right (rect ${JSON.stringify(ht.fs.rect)})`); }
  }
  return ok;
};

let settledAllOk = true;
const hitAllOk = []; // the per-state hit-test verdicts (the --hit's PASS/FAIL)
const trigger = async (label, metrics) => {
  console.log(`\n  … trigger ${label}: viewport -> ${metrics.width}x${metrics.height} @${metrics.deviceScaleFactor}`);
  await send('Emulation.setDeviceMetricsOverride', { ...metrics });
  await sleep(20);
  await sample(label + ' +20ms');
  await sleep(130);
  await sample(label + ' +150ms');
  await sleep(1050);
  const settled = await sample(label + ' +1200ms (settled)');
  if (!settled.ok) settledAllOk = false;
  await reslogDrain();
  return settled;
};

if (scale) {
  const baseline = await sample('baseline (menu, 390x844 @3)');
  if (!baseline.ok) settledAllOk = false;
  // SUSPECT 1 (the spec's first): the SIZE change — the fullscreen
  // exit's viewport change (the browser's UI returns, the viewport
  // shrinks / grows). A `resize` event fires; if the listener measured
  // before the layout settled, the backing store is the stale box.
  await trigger('size 800x600 @3', { width: 800, height: 600, deviceScaleFactor: 3, mobile: true });
  // SUSPECT 2 (the spec's second): the DPR-ONLY change (same CSS size,
  // the Android `devicePixelRatio` change across a fullscreen
  // transition). The probe's measured fact: NO `resize` fires for it —
  // the reslog stays empty, and pre-fix the invariant stays violated
  // forever (the "never recovers"); the fix's per-frame DPR sensor
  // restores it within a frame of the change.
  await trigger('dpr-only 800x600 @2.5', { width: 800, height: 600, deviceScaleFactor: 2.5, mobile: true });
}

if (hit) {
  // MENU STATE hit-test (item 1: the proof covers BOTH states — the
  // button is present in the menu too, top-right, out of the RotateCW
  // corner; the menu's top-right corner carries no game-drawn control —
  // the 9-zone grid's top band is a gesture zone, not a button).
  hitAllOk.push(await hitTest('menu state (800x600 @2.5)'));

  // Drive to the game state (the 032/046 flow): the boot screens' Enters
  // reach the name entry (or the profile select — a returned player's
  // persisted profile, the 048's IDBFS persistence), a name (a no-op
  // key in the select state), the profile-creating / the level's Enter,
  // the level select, the level start.
  const steps = ['Enter', 'Enter', 'C', 'Enter', 'Enter', 'Enter'];
  for (const k of steps) {
    const r = await press(k);
    console.log(`  … ${k}: ${r.before ?? '(n/a)'} -> ${r.now ?? '(n/a)'}`);
  }
  await sleep(500); // the frame after the transition (the overlay's gate)

  // GAME STATE hit-test (the collision resolution's proof — the F31 row's
  // three top-right buttons + the button's below-the-band spot).
  hitAllOk.push(await hitTest('game state (800x600 @2.5)'));

  // The invariant pair IN the game state (the operator's exact sequence:
  // the viewport changes while a level runs — the fullscreen exit on the
  // phone).
  const g = await sample('game baseline (800x600 @2.5)');
  if (!g.ok) settledAllOk = false;
  await trigger('game: fullscreen exit 390x844 @3', { ...BASE });
}

if (wiring) {
  // THE DESYNC WIRING (item 2's proof, the 046's synthetic-click pattern
  // + the operator's external-exit half): the #fs.click()'s direction is
  // the browser's ACTUAL state (pre-fix: the config's value — the stale-
  // after-external-exit no-op), and the `fullscreenchange` re-sync writes
  // the config from `document.fullscreenElement`.
  console.log('\n  … wiring: synthetic #fs.click() (the requestFullscreen path)');
  await evExpr(`const fs = document.getElementById('fs'); fs && fs.click(); return true;`);
  await sleep(400);
  const fsAfterClick = await evExpr(`return { element: !!document.fullscreenElement, enabled: document.fullscreenEnabled };`);
  console.log(`  [wiring] after click: fullscreenElement=${fsAfterClick.element} enabled=${fsAfterClick.enabled}`);
  const clickLine = consoleLogs.filter((l) => /\[oh-web-fs\] fullscreen ->/.test(l)).pop();
  console.log(`  [wiring] click line: ${clickLine || '(no [oh-web-fs] fullscreen -> line — the click did not reach the sink)'}`);
  if (fsAfterClick.element) {
    // The CDP GRANTED the transient user activation (046's observed
    // run-dependent grant) — a REAL fullscreen is on: the EXTERNAL exit
    // (the operator's "the browser itself gets out of fullscreen") via a
    // real CDP Escape keypress (not the button — the button is the game's
    // own path; Esc is the browser's). The re-sync line is the desync's
    // close (the config true -> false, from the browser's actual state).
    console.log('  … wiring: CDP Escape (the EXTERNAL exit — the operator\'s "the browser itself gets out of fullscreen")');
    await press('Escape');
    await sleep(600);
    const fsAfterEsc = await evExpr(`return !!document.fullscreenElement;`);
    console.log(`  [wiring] after Esc: fullscreenElement=${fsAfterEsc}`);
    const resyncLine = consoleLogs.filter((l) => /\[oh-web-fs\] fullscreenchange -> false \(config true -> false/.test(l)).pop();
    console.log(`  [wiring] re-sync line: ${resyncLine || '(no [oh-web-fs] fullscreenchange -> false (config true -> false) line — the re-sync did not land)'}`);
    hitAllOk.push(Boolean(resyncLine) && fsAfterEsc === false);
  } else {
    // The CDP DENIED the activation (the `promise rejected` line — the
    // task's honest limit, the 041/046 twin): the transition's real grant
    // is the operator's tap on the phone. The wiring's proof here is the
    // click line alone (the API was called with the browser-state-derived
    // direction).
    const rejectLine = consoleLogs.filter((l) => /\[oh-web-fs\] promise rejected/.test(l)).pop();
    console.log(`  [wiring] (no fullscreen grant — headless CDP's honest limit; the operator's real tap grants it)`);
    console.log(`  [wiring] rejection line: ${rejectLine || '(no [oh-web-fs] promise rejected line either — check the click line above)'}`);
  }
}

// The port's own account of the fullscreen wiring (the `[oh-web-fs]`'s
// console-visible diagnostic lines — the Rust stdio's black hole, the
// `oh-game` log sink's browser-console route).
const fsLines = consoleLogs.filter((l) => /\[oh-web-fs\]/.test(l));
console.log(`\nweb-fs-probe: [oh-web-fs] lines:`);
if (fsLines.length) { for (const l of fsLines) console.log(`    ${l}`); }
else { console.log(`    (no [oh-web-fs] lines — the #fs wiring was not exercised in this run)`); }

const hitOk = hitAllOk.length > 0 ? hitAllOk.every(Boolean) : true;
if (hit) console.log(`web-fs-probe: hit-tests ${hitOk ? 'PASS' : 'FAIL'}`);
console.log(`web-fs-probe: scale invariant ${settledAllOk ? 'PASS' : 'FAIL'}`);
process.exitCode = hitOk && settledAllOk && errors.length === 0 ? 0 : 1;
if (errors.length) console.log(`  page exceptions: ${errors.join(' | ')}`);
chrome.kill('SIGKILL');

