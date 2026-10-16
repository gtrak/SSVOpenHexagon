// Issue 29 — the module-by-module parity audit workflow.
//
// WHY: the marker-grep inventory (issue 28) finds only the deviations someone
// already DOCUMENTED. The 9-zone touch grid proved the dangerous ones are the
// UNDOCUMENTED ones: C++ behavior with no Rust code at all. This workflow reads
// BOTH sides of each module pair and reports what the port is MISSING.
//
// Shape, per module pair (12 pairs):
//   <key>-cpp   (scout)    read the C++, produce a mechanical behavior inventory
//   <key>-rust  (scout)    read the Rust, produce the implemented-behavior list
//   <key>-diff  (worker)   adversarially diff the two, WRITE the full report to
//                          .agents/audits/<key>.md, return a short summary only
//
// Bounded waves: 2 pairs per wave (4 concurrent scouts), so the local model
// server is never asked for 24 simultaneous children. Every diff report is
// written to disk by the child, so nothing is lost if a stage fails; the
// workflow returns only compact summaries and the parent reads the files.
//
// All paths in PAIRS were verified to exist before this workflow was committed.

const PAIRS = [
  { key: "menu", cpp: "src/SSVOpenHexagon/Core/MenuGame.cpp",
    rust: "crates/oh-menu/src/menu_game.rs crates/oh-menu/src/menu.rs crates/oh-menu/src/layout.rs crates/oh-menu/src/dialog_box.rs crates/oh-menu/src/seams.rs crates/oh-app/src/menu_draw.rs",
    note: "the state machine (States::*), event handlers (onEvent Key/Mouse/Joystick/Text), the 9-zone touch grid, the action methods (ok/left/right/up/down and the *Action family), the draw methods, favorites, mouseHovering tracking" },
  { key: "game", cpp: "src/SSVOpenHexagon/Core/HexagonGame.cpp",
    rust: "crates/oh-game/src/hexagon_game.rs",
    note: "the death sequence (death_* family), scoring/PB paths, replay save/send, rich-presence updates, status/color getters, the message queue" },
  { key: "update", cpp: "src/SSVOpenHexagon/Core/HGUpdate.cpp",
    rust: "crates/oh-game/src/update.rs",
    note: "the update loop, spawn/pattern logic, collision, powerups, beat/pulse timing, touch controls, and the ImGui-console hook points (Android-excluded in C++)" },
  { key: "graphics", cpp: "src/SSVOpenHexagon/Core/HGGraphics.cpp",
    rust: "crates/oh-render/src/graphics.rs crates/oh-app/src/windowed.rs",
    note: "every draw stage, camera setup, the screenshot path, the drawImguiLuaConsole tail" },
  { key: "assets", cpp: "src/SSVOpenHexagon/Global/Assets.cpp",
    rust: "crates/oh-assets/src/assets.rs crates/oh-assets/src/asset_storage.rs",
    note: "pack/level discovery, asset loading, texture/music/sound tables, the profile-editor operations, error collection (the load-results screen)" },
  { key: "config", cpp: "src/SSVOpenHexagon/Global/Config.cpp",
    rust: "crates/oh-config/src/config.rs",
    note: "every config key, persistence, resolution handling, the window setters" },
  { key: "replay", cpp: "src/SSVOpenHexagon/Core/Replay.cpp",
    rust: "crates/oh-replay/src/lib.rs",
    note: "the byte-level format, compression, the packet codec" },
  { key: "lua", cpp: "src/SSVOpenHexagon/Core/LuaScripting.cpp",
    rust: "crates/oh-lua/src/lib.rs",
    note: "the registered Lua function set (count and names), callback dispatch, exception surfaces" },
  { key: "client", cpp: "src/SSVOpenHexagon/Core/HexagonClient.cpp",
    rust: "crates/oh-net/src/lib.rs",
    note: "the online flow, kick handling, leaderboard cache invalidation, profile/database operations" },
  { key: "joystick", cpp: "src/SSVOpenHexagon/Core/Joystick.cpp",
    rust: "crates/oh-config/src/joystick.rs",
    note: "the state machine is reportedly ported — determine whether anything FEEDS it (a poller) and whether the calling sites are dispatched; report that explicitly" },
  { key: "main", cpp: "src/SSVOpenHexagon/Core/main.cpp",
    rust: "crates/oh-app/src/client.rs crates/oh-app/src/compose.rs crates/oh-app/src/dispatch.rs",
    note: "the CLI, the boot order, headless/server modes, the window setters (the mustRecreate family)" },
  { key: "dialog", cpp: "src/SSVOpenHexagon/Core/HexagonDialogBox.cpp",
    rust: "crates/oh-menu/src/dialog_box.rs",
    note: "the dialog state machine, input gating (setIgnoreAllInputs), the draw" },
];

function cppTask(p) {
  return [
    "You are auditing ONE C++ module of a game being ported to Rust (repo /home/gary/dev/SSVOpenHexagon).",
    "Read this C++ module COMPLETELY: " + p.cpp,
    "Audit focus: " + p.note + ".",
    "",
    "Produce a BEHAVIOR INVENTORY - mechanical, exhaustive, not a prose summary:",
    "1. Every member function: name, what it DOES in one line, and its file:line.",
    "2. Every event handler / subscription / risingEdge poll: the exact trigger and the effect.",
    "3. Every global / file-static / static member it READS (the C++ global mutable state) and every one it WRITES.",
    "4. Every side effect on shared state: files written, network calls, sounds played, screenshots taken, window operations.",
    "5. Every state transition it performs (state-machine edges).",
    "",
    "Output a markdown checklist grouped by function. Do NOT discuss Rust: this list is the ground truth",
    "the Rust side will be compared against. Exhaustiveness matters far more than brevity.",
  ].join("\n");
}

function rustTask(p) {
  return [
    "You are auditing the RUST side of a C++ -> Rust port (repo /home/gary/dev/SSVOpenHexagon).",
    "Read these Rust sources COMPLETELY: " + p.rust,
    "They port: " + p.cpp + " (audit focus: " + p.note + ").",
    "",
    "Produce an IMPLEMENTED BEHAVIOR LIST - mechanical and exhaustive:",
    "1. Every fn (pub and private): name, one line on behavior, and its file:line.",
    "2. Every place the port READS input / global state (the analogue of the C++ globals).",
    "3. Everything explicitly stubbed / no-op / TODO: quote the marker text and give file:line.",
    "4. Any behavior the Rust side has that the C++ side does not have.",
    "",
    "Do NOT discuss the C++ side. This list is compared against the C++ inventory.",
  ].join("\n");
}

function diffTask(p, cppInventory, rustInventory) {
  return [
    "You are the ADVERSARIAL DIFF for ONE module pair of a 1:1 C++ -> Rust port.",
    "Pair: " + p.cpp + "  ->  " + p.rust,
    "Audit focus: " + p.note + ".",
    "",
    "Below are two independent behavior inventories, each produced by reading one side.",
    "Your job: find everything the C++ does that the Rust does NOT do.",
    "",
    "Rules:",
    "- Report MISSING BEHAVIOR, not missing comments. Behavior is missing when the C++ performs it and",
    "  no Rust code performs it - even if nobody documented the omission. That is the whole point.",
    "- VERIFY absence yourself with a grep (you have read/grep/bash when in doubt) and include the exact",
    "  command plus its result as the absence proof for each missing item.",
    "- Do NOT count deliberate platform seams as missing (feature-gated Steam/Discord/network, headless",
    "  no-ops, the Android-excluded ImGui console, test doubles). List those separately with their marker.",
    "- Risk per item: uservisible-gameplay | uservisible-cosmetic | determinism | data-format | internal | none.",
    "",
    "WRITE THE FULL REPORT to .agents/audits/" + p.key + ".md (create the directory if needed) with these sections:",
    "## MISSING   - a table, highest risk first: item | C++ evidence (file:line) | absence proof (command + result) | risk",
    "## SEAMS     - accepted deviations with the marker that declares them",
    "## DIFFERENT - present but renamed or behaviorally different",
    "## VERDICT   - one line: does this pair pass 1:1; if not, the single most important gap",
    "",
    "Then RETURN (as your final message) a SHORT summary only: the verdict line, the count of missing",
    "items by risk, and the top 5 gaps one line each. The full table lives in the file.",
    "",
    "Be exhaustive in the FILE: a short report that misses a load-bearing gap is a failure.",
    "",
    "=== C++ INVENTORY ===",
    cppInventory,
    "",
    "=== RUST INVENTORY ===",
    rustInventory,
  ].join("\n");
}

const WAVE = 2;
const summaries = [];
for (let i = 0; i < PAIRS.length; i += WAVE) {
  const batch = PAIRS.slice(i, i + WAVE);
  const specs = [];
  for (const p of batch) {
    specs.push({ key: p.key + "-cpp", agent: "scout", task: cppTask(p) });
    specs.push({ key: p.key + "-rust", agent: "scout", task: rustTask(p) });
  }
  const inventories = await runs.all(specs);
  const diffSpecs = batch.map((p, j) => ({
    key: p.key + "-diff",
    agent: "worker",
    task: diffTask(p, inventories[2 * j].output, inventories[2 * j + 1].output),
  }));
  const diffs = await runs.all(diffSpecs);
  for (let j = 0; j < batch.length; j++) {
    summaries.push({ pair: batch[j].key, ok: diffs[j].ok, summary: diffs[j].output });
  }
}
return summaries;
