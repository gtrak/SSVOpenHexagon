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
//
// The port logs `status booted=…;exit_code=…;ticks=…;state=menu:<States|game>` through its
// log_sink (oh-web installs a console.log sink), so the state timeline below is the port's own
// account of itself — not an inference from pixels.
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
const shotPath = (argv.find((a) => a.startsWith('--shot=')) || '').slice(7) || '/tmp/web-probe.png';

const CHROME = process.env.OH_CHROME
  || ['/home/gary/.agent-browser/browsers/chrome-148.0.7778.97/chrome',
      '/usr/bin/google-chrome', '/usr/bin/chromium'].find((p) => existsSync(p));
if (!CHROME) { console.error('web-probe: no chrome found; set OH_CHROME=/path/to/chrome'); process.exit(1); }
const PORT = Number(process.env.OH_CDP_PORT || 9335);

const chrome = spawn(CHROME, ['--headless=new', `--remote-debugging-port=${PORT}`, '--no-sandbox',
  '--disable-dev-shm-usage', '--use-angle=swiftshader', '--enable-unsafe-swiftshader',
  '--window-size=1280,720', '--no-first-run', `--user-data-dir=/tmp/oh-web-probe-profile`, 'about:blank'],
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
await send('Page.navigate', { url });

const stateOf = async () => {
  const r = await send('Runtime.evaluate', { expression: `(typeof Module!=='undefined'&&Module.status)?Module.status():null`, returnByValue: true });
  return r.result?.result?.value ?? null;
};
const press = async (key) => {
  const vk = key === 'Enter' ? 13 : key === 's' ? 83 : key.toUpperCase().charCodeAt(0);
  const code = key.length === 1 ? `Key${key.toUpperCase()}` : key;
  for (const type of ['keyDown', 'keyUp']) {
    await send('Input.dispatchKeyEvent', { type, key, code, windowsVirtualKeyCode: vk, nativeVirtualKeyCode: vk, text: type === 'keyDown' && key.length === 1 ? key : key === 'Enter' ? '\r' : undefined });
  }
};

const perKey = Math.max(4, Math.floor(secs / (keys.length + 1)));
await sleep(perKey * 1000);
for (const k of keys) { console.log(`  … pressing ${k}`); await press(k); await sleep(perKey * 1000); }
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
console.log(`  probe         : ${await stateOf() ?? '(Module.status not reachable via evaluate)'}`);
console.log(`  screenshot    : ${shotPath}`);
console.log(`  non-200       : ${bad.length ? bad.join(' | ') : 'none'}`);
console.log(`  failed loads  : ${failed.length ? failed.join(' | ') : 'none'}`);
console.log(`  console errors: ${errors.length ? errors.map((e) => e.slice(0, 160)).join(' | ') : 'none'}`);
chrome.kill('SIGKILL');

const realErrors = errors.filter((e) => !/favicon/i.test(e));
const realBad = bad.filter((b) => !/favicon/.test(b));
process.exit(boots.length > 0 && realErrors.length === 0 && realBad.length === 0 ? 0 : 1);
