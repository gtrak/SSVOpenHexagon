#!/usr/bin/env node
// tools/web-persist-probe.mjs — end-to-end proof of issue 048 (F34 / D-039):
// the wasm build's user data (profiles, config, replays) persists in
// IndexedDB across a page reload, through emscripten IDBFS.
//
// What it drives (a REAL headless Chrome over CDP, SwiftShader GL — the
// same recipe as tools/web-probe.mjs; no npm dependencies, node >=22):
//
//   PHASE 1 (fresh browser profile — empty IndexedDB):
//     1. boot → LoadingScreen → (key) EpilepsyWarning → (key) the C++'s
//        own FIRST-RUN profile creation (menu:ETLPNewBoot — proves the
//        fresh-user path still works with the persistence gate in place);
//     2. type the profile name "probe" + Enter → the profile file lands in
//        /userdata/Profiles/ through the page's symlink (autoPersist puts
//        it in IndexedDB at the end of the frame);
//     3. navigate to MOpts → GAMEPLAY → toggle "autorestart" (config
//        auto_restart false → true);
//     4. dispatch a synthetic `pagehide` — the page's way-out glue runs
//        `mod.save_session()` (the C++ mainClient's exit-save: pSaveAll +
//        saveConfig, the D-029 gap this issue closes) + the final
//        `FS.syncfs(false)` flush;
//     5. READ IndexedDB DIRECTLY (database "/userdata", store "FILE_DATA"
//        — emscripten IDBFS's own names, verified against the emscripten
//        6.0.12 source): assert the profile file + the config.json with
//        `"auto_restart": true` are in the durable store, NOT just MEMFS.
//
//   PHASE 2 (Page.reload in the SAME tab — the MEMFS is re-preloaded from
//   oh_web.data, i.e. the shipped _RELEASE content comes back FRESH; only
//   IndexedDB survives):
//     6. boot → LoadingScreen → (key) EpilepsyWarning → (key) the boot
//        MUST land on menu:SLPSelectBoot (the profile list), NOT
//        menu:ETLPNewBoot — the profile persisted AND the boot skipped
//        name entry (the issue's acceptance row);
//     7. read /RELEASE/config.json THROUGH THE ENGINE'S OWN PATH (the
//        module's FS, the page's symlink into IDBFS): assert
//        `"auto_restart": true` survived (the in-session change, saved by
//        the pagehide glue).
//
//   node tools/web-persist-probe.mjs [site-dir]   # default: crates/oh-web/site
//
// Exit: 0 only when every checkpoint passes; 1 otherwise. A checkpoint
// failure prints what the probe observed (the status timeline + the
// IndexedDB contents) so a failure is debuggable from the log.

import { spawn } from 'node:child_process';
import { setTimeout as sleep } from 'node:timers/promises';
import { writeFileSync, existsSync, rmSync } from 'node:fs';

const siteDir = process.argv[2] || 'crates/oh-web/site';
if (!existsSync(`${siteDir}/oh_web.wasm`)) {
  console.error(`web-persist-probe: ${siteDir}/oh_web.wasm missing — build the wasm target first (the root .cargo/config.toml's [target.wasm32-unknown-emscripten] flags)`);
  process.exit(1);
}

const CHROME = process.env.OH_CHROME
  || ['/home/gary/.agent-browser/browsers/chrome-148.0.7778.97/chrome',
      '/usr/bin/google-chrome', '/usr/bin/chromium'].find((p) => existsSync(p));
if (!CHROME) { console.error('web-persist-probe: no chrome found; set OH_CHROME=/path/to/chrome'); process.exit(1); }
const CDP_PORT = Number(process.env.OH_CDP_PORT || 9336);
const HTTP_PORT = Number(process.env.OH_HTTP_PORT || 8766);
const PROFILE_DIR = `/tmp/oh-web-persist-probe-${process.pid}`;
const SHOT_DIR = process.env.OH_PERSIST_SHOTS || '/tmp';
rmSync(PROFILE_DIR, { recursive: true, force: true });

// ---- the static server (the probe owns it: spawn, wait, kill) ----
const server = spawn('python3', ['-m', 'http.server', String(HTTP_PORT), '--bind', '127.0.0.1', '--directory', siteDir],
  { stdio: ['ignore', 'ignore', 'ignore'] });
const httpUp = async () => {
  for (let i = 0; i < 40; i++) {
    try { const r = await fetch(`http://127.0.0.1:${HTTP_PORT}/index.html`); if (r.ok) return true; } catch { /* not up yet */ }
    await sleep(250);
  }
  return false;
};
if (!(await httpUp())) { server.kill('SIGKILL'); console.error('web-persist-probe: the static server never came up'); process.exit(1); }

// ---- headless Chrome (a FRESH profile: empty IndexedDB = a fresh user) ----
const chrome = spawn(CHROME, ['--headless=new', `--remote-debugging-port=${CDP_PORT}`, '--no-sandbox',
  '--disable-dev-shm-usage', '--use-angle=swiftshader', '--enable-unsafe-swiftshader',
  '--window-size=1280,720', '--no-first-run', `--user-data-dir=${PROFILE_DIR}`, 'about:blank'],
  { stdio: ['ignore', 'ignore', 'ignore'] });

const http = async (p, i) => (await fetch(`http://127.0.0.1:${CDP_PORT}${p}`, i)).json();
let up = false;
for (let i = 0; i < 60 && !up; i++) { try { await http('/json/version'); up = true; } catch { await sleep(500); } }
if (!up) { chrome.kill('SIGKILL'); server.kill('SIGKILL'); console.error('web-persist-probe: chrome never opened its debug port'); process.exit(1); }

const target = await http(`/json/new?${encodeURIComponent(`http://127.0.0.1:${HTTP_PORT}/`)}`, { method: 'PUT' });
const ws = new WebSocket(target.webSocketDebuggerUrl);
let id = 0; const pending = new Map();
const logs = []; const errors = [];
ws.addEventListener('message', (ev) => {
  const m = JSON.parse(ev.data);
  if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); return; }
  const p = m.params || {};
  if (m.method === 'Runtime.consoleAPICalled') {
    const text = (p.args || []).map((a) => a.value ?? a.description ?? '').join(' ');
    if (p.type === 'error') errors.push(`[error] ${text}`); else logs.push(text);
  } else if (m.method === 'Runtime.exceptionThrown') {
    errors.push(`[exception] ${p.exceptionDetails?.exception?.description || p.exceptionDetails?.text}`);
  }
});
const send = (method, params = {}) => new Promise((r) => { const i = ++id; pending.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
await new Promise((r) => ws.addEventListener('open', r));
await send('Runtime.enable'); await send('Page.enable');
await send('Network.setCacheDisabled', { cacheDisabled: true });
await send('Page.navigate', { url: `http://127.0.0.1:${HTTP_PORT}/` });

// ---- helpers ----
const lastStatus = () => {
  for (let i = logs.length - 1; i >= 0; i--) {
    const m = /^status (booted=[^\n]*)$/.exec(logs[i]);
    if (m) return m[1];
  }
  return null;
};
const stateOf = (s) => {
  const m = /state=([^\s;]+)/.exec(s || '');
  return m ? m[1] : null;
};
// Wait until the status's `state=` field equals `want` (or the deadline).
const waitState = async (want, secs) => {
  const t0 = Date.now();
  while (Date.now() - t0 < secs * 1000) {
    if (stateOf(lastStatus()) === want) return true;
    await sleep(500);
  }
  return false;
};
// Wait until a status line exists at all (the module is up; `state=` may be
// the pre-composition sentinel while the persistence gate is pending).
const waitAnyStatus = async (secs) => {
  const t0 = Date.now();
  while (Date.now() - t0 < secs * 1000) {
    if (lastStatus()) return true;
    await sleep(500);
  }
  return false;
};
// A key press: the CDP keyDown + keyUp pair (the menu reacts to the release
// half on the boot screens, to the press half for the binds; sending both is
// the real key's shape). `text` = the DOM `key` character (TextEntered).
const press = async (key, { text = undefined } = {}) => {
  // NOTE (the probe's own quirk, verified against the C++): the key that
  // CLOSES the boot screens must not be a typeable letter — the keyDown's
  // TextEntered lands in `enteredChars` and the C++'s `playLocally()`
  // clears only `enteredStr` (MenuGame.cpp:1783-1789), so a 'w' close
  // would type a 'w' into the fresh profile name ("WPROBE" — the C++
  // leaks it too: the `isEnteringText`-gated accumulation, MenuGame.cpp:
  // 2862-2878, runs on the next update once the state is ETLPNewBoot).
  // Space closes the boot screens, types nothing (not alphanumeric /
  // punctuation, the accumulation's filter), and has no menu bind in the
  // boot states (the menu's ok is the hardcoded Enter; Tid::Select =
  // Space is a gameplay bind, not registered in `register_menu_binds`).
  const vk = key === 'Enter' ? 13 : key === ' ' ? 32 : key.length === 1 ? key.toUpperCase().charCodeAt(0) : key === 'ArrowDown' ? 40 : key === 'ArrowUp' ? 38 : 0;
  const code = key === 'Enter' ? 'Enter' : key === ' ' ? 'Space' : key.length === 1 ? `Key${key.toUpperCase()}` : key;
  // HOLD the key between keyDown and keyUp (a frame's worth): the C++'s
  // menu binds are the SSVStart Manager's per-frame trigger POLLING
  // (`game.addInput`, MenuGame.cpp:912-1018) — a key tapped entirely
  // inside one frame (press + release between two input snapshots)
  // leaves no trace in the per-frame snapshot (the port's 025c
  // documented finding: "a fast hardware key tap … leaves no trace in
  // the per-frame InputState snapshot"). A held key is the real
  // key-press shape (a human tap is ~100 ms+ = several frames), and the
  // C++ would miss the fast tap too — the port is faithful, the probe
  // must press like a human.
  await send('Input.dispatchKeyEvent', {
    type: 'keyDown', key, code,
    windowsVirtualKeyCode: vk, nativeVirtualKeyCode: vk,
    text: text,
  });
  await sleep(120);
  await send('Input.dispatchKeyEvent', {
    type: 'keyUp', key, code,
    windowsVirtualKeyCode: vk, nativeVirtualKeyCode: vk,
  });
  await sleep(400); // let the rAF loop drain the queued events (a frame's worth)
};
// Advance one boot screen (LoadingScreen → EpilepsyWarning → the profile
// state): press + wait, with retries — the first key right after module
// boot can land in a slow SwiftShader frame; a real user just presses
// again.
const advanceBoot = async (wantState, tries = 5) => {
  for (let i = 0; i < tries; i++) {
    await press(' ');
    if (await waitState(wantState, 8)) return true;
  }
  return false;
};
const evalJs = async (expression) => {
  const r = await send('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true });
  if (r.result?.exceptionDetails) return { error: r.result.exceptionDetails.exception?.description || r.result.exceptionDetails.text };
  return { value: r.result?.result?.value ?? null };
};
const shot = async (name) => {
  const s = await send('Page.captureScreenshot', { format: 'png' });
  if (s.result?.data) writeFileSync(`${SHOT_DIR}/${name}.png`, Buffer.from(s.result.data, 'base64'));
};
// Read emscripten IDBFS's durable store directly: database "/userdata"
// (the mount point — IDBFS names the DB after the mountpoint), store
// "FILE_DATA" (IDBFS.DB_STORE_NAME, emscripten 6.0.12), version 21
// (IDBFS.DB_VERSION). Keys are the absolute paths under /userdata.
const IDB_READ = `
(async () => {
  const open = (db) => new Promise((res, rej) => {
    const r = indexedDB.open(db, 21);
    r.onsuccess = () => res(r.result);
    r.onerror = () => rej(new String(r.error));
    r.onupgradeneeded = () => {};
  });
  const getKeys = (s) => new Promise((res, rej) => { const q = s.getAllKeys(); q.onsuccess = () => res(q.result); q.onerror = () => rej(new String(q.error)); });
  const getVal = (s, k) => new Promise((res, rej) => { const q = s.get(k); q.onsuccess = () => res(q.result); q.onerror = () => rej(new String(q.error)); });
  const dec = (b) => (b ? new TextDecoder().decode(b) : null);
  let db;
  try { db = await open('/userdata'); } catch (e) { return { missing: true, why: String(e) }; }
  let s;
  try { s = db.transaction('FILE_DATA', 'readonly').objectStore('FILE_DATA'); } catch (e) { return { missing: true, why: 'no FILE_DATA store yet' }; }
  const keys = (await getKeys(s)).map(String);
  const cfg = keys.includes('/userdata/config.json') ? await getVal(s, '/userdata/config.json') : null;
  const profKey = keys.find((k) => k.startsWith('/userdata/Profiles/') && k.endsWith('.json')) || null;
  const prof = profKey ? await getVal(s, profKey) : null;
  return {
    keys,
    config: dec(cfg && cfg.contents),
    profileKey: profKey,
    profile: dec(prof && prof.contents),
  };
})()`;

// ---- the checkpoints ----
const results = [];
const check = (name, ok, detail) => {
  results.push({ name, ok, detail });
  console.log(`  [${ok ? 'PASS' : 'FAIL'}] ${name}${detail ? ` — ${detail}` : ''}`);
};
// The known pre-existing console noise (NOT a gate for this lane): the
// 041 oh-audio volume diagnostics — `eprintln!` at config-apply
// (crates/oh-audio/src/audio.rs:398/:418), CDP-classified as console
// `error` type, present in EVERY web run (the 041 probe's own output
// carries them).
const knownNoise = (l) => /\[oh-audio\] set (sound|music) volume/.test(l);

console.log(`web-persist-probe: ${siteDir} → http://127.0.0.1:${HTTP_PORT}/ (chrome ${CHROME})`);

// =============== PHASE 1 — fresh user: create + change, then way-out ===============
console.log('\nPHASE 1 (fresh IndexedDB):');

// 1. Boot through the two boot screens. The composition boots from the
//    persistence gate's callback (after the FS.syncfs load), so the first
//    status line may pre-date the composition — wait for the menu state.
const t0 = Date.now();
if (!(await waitState('menu:LoadingScreen', 180))) {
  check('boot to LoadingScreen', false, `timed out after ${Math.round((Date.now() - t0) / 1000)}s; last status: ${lastStatus()}`);
  console.log(logs.slice(-15).join('\n'));
  chrome.kill('SIGKILL'); server.kill('SIGKILL');
  process.exit(1);
}
check('boot to LoadingScreen (fresh profile)', true, `${Math.round((Date.now() - t0) / 1000)}s`);
if (!(await advanceBoot('menu:EpilepsyWarning'))) { check('advance to EpilepsyWarning', false, `last: ${lastStatus()}`); chrome.kill('SIGKILL'); server.kill('SIGKILL'); process.exit(1); }
check('advance to EpilepsyWarning', true);
if (!(await advanceBoot('menu:ETLPNewBoot'))) {
  check('fresh user → profile creation (ETLPNewBoot)', false, `landed on ${stateOf(lastStatus())} instead — the boot gate should reach the C++'s first-run state`);
  chrome.kill('SIGKILL'); server.kill('SIGKILL'); process.exit(1);
}
check('fresh user → profile creation (ETLPNewBoot)', true);

// 2. Type the profile name + Enter. (TextEntered rides the keyDown half;
//    the name avoids the menu-bound keys — none of p/r/o/b/e is a menu
//    bind, and the C++'s is_entering_text gate guards the Escape/Exit
//    binds anyway.)
for (const ch of ['p', 'r', 'o', 'b', 'e']) await press(ch, { text: ch });
await sleep(300);
await press('Enter', { text: '\r' });
if (!(await waitState('menu:SMain', 20))) {
  check('profile "probe" created (→ SMain)', false, `last: ${lastStatus()}`);
  chrome.kill('SIGKILL'); server.kill('SIGKILL'); process.exit(1);
}
check('profile "probe" created (→ SMain)', true);

// 3. Navigate SMain → OPTIONS → MOpts → GAMEPLAY → toggle "autorestart".
//    SMain items: LEVEL SELECT(0), LOCAL PROFILES(1), ONLINE(2),
//    OPTIONS(3), EXIT(4) — S (Tid::Down) ×3, Enter (the hardcoded Enter
//    ok_action). MOpts "options" category: GAMEPLAY(0)…Enter → the
//    "gameplay" category: autorestart(0) — D (Tid::RotateCW) toggles it
//    false→true (the defaults config's auto_restart is false).
for (let i = 0; i < 3; i++) await press('s');
await press('Enter', { text: '\r' });
if (!(await waitState('menu:MOpts', 15))) { check('open MOpts', false, `last: ${lastStatus()}`); chrome.kill('SIGKILL'); server.kill('SIGKILL'); process.exit(1); }
check('open MOpts (options category)', true);
await press('Enter', { text: '\r' }); // GAMEPLAY → the gameplay category
await sleep(500);
await shot('persist-mopts-gameplay');
await press('d'); // autorestart: false → true
await sleep(1500); // the autoPersist's end-of-frame flush of the profile write

// 4. The way-out: the synthetic pagehide fires the page's flushSession
//    (mod.save_session() — the C++ mainClient's exit-save — + the final
//    FS.syncfs(false)).
const ph = await evalJs(`window.dispatchEvent(new Event('pagehide')).constructor === 'Event'`);
check('dispatch pagehide (the way-out glue)', ph.value === true || ph.error === undefined, ph.error || 'synthetic pagehide dispatched');
await sleep(1200); // save_session's writes + the syncfs(false) flush settle

// 5. Read IndexedDB directly — the DURABLE store, not MEMFS.
const idb1 = (await evalJs(IDB_READ)).value;
if (idb1?.missing) {
  check('IndexedDB durable store reachable', false, idb1.why);
  console.log(logs.slice(-15).join('\n'));
  chrome.kill('SIGKILL'); server.kill('SIGKILL'); process.exit(1);
}
check('IndexedDB durable store reachable', true, `${idb1.keys.length} entries: ${idb1.keys.join(', ')}`);
check('profile persisted to IndexedDB', /\/userdata\/Profiles\/probe\.json$/.test(idb1.profileKey || ''),
  `key=${idb1.profileKey ?? 'none'}${idb1.profile ? `, bytes=${idb1.profile.length}` : ''}`);
const cfgAuto = idb1.config ? /"auto_restart"\s*:\s*true/.test(idb1.config) : false;
check('config persisted with the changed setting (auto_restart: true)', cfgAuto,
  idb1.config ? `config.json ${idb1.config.length}B in the durable store` : 'no /userdata/config.json entry — the pagehide save did not reach IndexedDB');

// =============== PHASE 2 — reload: the MEMFS comes back FRESH ===============
console.log('\nPHASE 2 (Page.reload — MEMFS re-preloaded, only IndexedDB survives):');
await send('Page.reload');
const t1 = Date.now();
if (!(await waitAnyStatus(60))) { check('reboot: module up', false, 'no status line within 60s after reload'); chrome.kill('SIGKILL'); server.kill('SIGKILL'); process.exit(1); }
check('reboot: module up', true);
if (!(await waitState('menu:LoadingScreen', 180))) {
  check('reboot to LoadingScreen', false, `timed out; last status: ${lastStatus()}`);
  console.log(logs.slice(-15).join('\n'));
  chrome.kill('SIGKILL'); server.kill('SIGKILL'); process.exit(1);
}
check('reboot to LoadingScreen', true, `${Math.round((Date.now() - t1) / 1000)}s (the 22 MB _RELEASE re-preload + the FS.syncfs load)`);
if (!(await advanceBoot('menu:EpilepsyWarning'))) { check('reboot: advance to EpilepsyWarning', false, `last: ${lastStatus()}`); chrome.kill('SIGKILL'); server.kill('SIGKILL'); process.exit(1); }
if (!(await advanceBoot('menu:SLPSelectBoot'))) {
  check('reboot: advance past EpilepsyWarning', false, `last: ${lastStatus()}`);
  chrome.kill('SIGKILL'); server.kill('SIGKILL'); process.exit(1);
}
const bootedState = await waitState('menu:SLPSelectBoot', 15) ? 'menu:SLPSelectBoot' : stateOf(lastStatus());
check('boot SKIPS name entry (→ SLPSelectBoot, not ETLPNewBoot)', bootedState === 'menu:SLPSelectBoot',
  `landed on ${bootedState} — a persisted profile must put the boot on the profile list`);
await shot('persist-reload-selectboot');

// 7. The changed setting survived — through the ENGINE'S OWN PATH (the
//    module's FS, /RELEASE/config.json is the page's symlink into IDBFS).
const cfgRead = await evalJs(`ohWebModule.FS.readFile('/RELEASE/config.json', { encoding: 'utf8' })`);
const cfgSurvived = cfgRead.value ? /"auto_restart"\s*:\s*true/.test(cfgRead.value) : false;
check('changed setting survived the reload (auto_restart: true via the engine path)', cfgSurvived,
  cfgRead.error ? `read failed: ${cfgRead.error}` : cfgRead.value ? `config.json ${cfgRead.value.length}B` : 'no config readable');

// And the profile file is still in the durable store after the reload's
// syncfs load (the restore direction proved its contents, this re-proves
// the key exists for the next session).
const idb2 = (await evalJs(IDB_READ)).value || {};
check('profile still in IndexedDB after the reload', /\/userdata\/Profiles\/probe\.json$/.test(idb2.profileKey || ''),
  `key=${idb2.profileKey ?? 'none'}`);

// ---- report ----
const persistOff = logs.filter((l) => /persistence OFF/.test(l));
console.log(`\npersistence gate log : ${logs.filter((l) => /user data restored|persisted user data ready|persistence OFF|save_session/.test(l)).join(' | ') || '(none)'}`);
const failed = results.filter((r) => !r.ok);
const realErrors = errors.filter((e) => !knownNoise(e));
console.log(`console errors        : ${realErrors.length ? realErrors.map((e) => e.slice(0, 160)).join(' | ') : 'none'}${errors.length ? ` (${errors.filter(knownNoise).length} known [oh-audio] volume diagnostics excluded)` : ''}`);
console.log(`\nweb-persist-probe: ${results.length - failed.length}/${results.length} checkpoints passed`);
console.log(`  screenshots        : ${SHOT_DIR}/persist-mopts-gameplay.png, ${SHOT_DIR}/persist-reload-selectboot.png`);
chrome.kill('SIGKILL'); server.kill('SIGKILL');
process.exit(failed.length === 0 && realErrors.length === 0 && persistOff.length === 0 ? 0 : 1);
