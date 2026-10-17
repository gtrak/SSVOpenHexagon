// Issue 29 — the module-by-module parity audit workflow (local model only).
//
// WHY: the marker-grep inventory (issue 28) finds only deviations someone
// already DOCUMENTED. The 9-zone touch grid proved the dangerous ones are the
// UNDOCUMENTED ones: C++ behavior with no Rust code at all. This audit reads
// BOTH sides of each module pair and reports what the port is MISSING.
//
// Shape: ONE child per module pair. The child reads the C++ module and its
// Rust counterpart itself, diffs them adversarially, WRITES the full report to
// .agents/audits/<key>.md, and returns a short summary. Bounded to 2
// concurrent children (operator directive), and every child is pinned to
// `worker` + `model: local/local` — builtin agents inherit the session model,
// which is NOT what we want here.
//
// 12 pairs x 1 child = 12 children, 2 at a time = 6 waves.

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

function auditTask(p) {
  return [
    "You are auditing ONE module pair of a C++ -> Rust 1:1 game port (repo /home/gary/dev/SSVOpenHexagon).",
    "The operator wants a literal 1:1 Rust port as the base, so ANY C++ behavior with no Rust counterpart is a defect.",
    "",
    "C++ module (read COMPLETELY): " + p.cpp,
    "Rust counterpart(s) (read COMPLETELY): " + p.rust,
    "Audit focus: " + p.note + ".",
    "",
    "METHOD (do this in order, and be mechanical):",
    "1. Read the C++ module fully. Build a BEHAVIOR INVENTORY: every member function (what it does, file:line);",
    "   every event handler / subscription / risingEdge poll and its trigger; every global / file-static /",
    "   static member it reads or writes; every side effect (files, network, sounds, screenshots, window ops);",
    "   every state transition.",
    "2. Read the Rust counterpart(s) fully. Build the IMPLEMENTED list: every fn (what it does, file:line),",
    "   every input/global it reads, every explicit stub/no-op/TODO (quote the marker), and anything present",
    "   in Rust that the C++ lacks.",
    "3. DIFF them adversarially. For each C++ behavior, decide: PRESENT (maybe renamed), MISSING, or SEAM",
    "   (a deliberate, documented platform gate).",
    "",
    "RULES:",
    "- Report MISSING BEHAVIOR, not missing comments. Behavior is missing when the C++ performs it and no",
    "  Rust code performs it — even if nobody documented the omission. That is the entire point.",
    "- VERIFY each suspected absence with a grep yourself; record the exact command and its result as proof.",
    "- Deliberate seams are NOT defects, but list them separately with the marker that declares them:",
    "  feature-gated Steam/Discord/network, headless no-ops, the Android-excluded ImGui console, test doubles.",
    "- Risk per missing item: uservisible-gameplay | uservisible-cosmetic | determinism | data-format | internal | none.",
    "- Cite file:line on BOTH sides for every finding. If a citation you read in a comment is wrong, say so",
    "  (drifting line numbers and uncited claims have already been found in this repo — flag them).",
    "",
    "OUTPUT: write the full report to .agents/audits/" + p.key + ".md (create the directory if needed) with:",
    "## MISSING   a table, highest risk first: item | C++ evidence (file:line) | absence proof (command + result) | risk",
    "## SEAMS     accepted deviations with their markers",
    "## DIFFERENT present but renamed or behaviorally different",
    "## VERDICT   one line: does this pair pass 1:1; if not, the single most important gap",
    "",
    "Do NOT modify any file other than .agents/audits/" + p.key + ".md. Do NOT run git commands that change state,",
    "DO NOT COMMIT ANYTHING (the orchestrator commits the reports), do NOT run cargo builds, and do NOT edit crates/",
    "or .agents/plans (another lane is writing crates/ right now). This is a READING audit plus one report file.",
    "",
    "Then RETURN as your final message a SHORT summary only: the VERDICT line, the missing-item count by risk,",
    "and the top 5 gaps (one line each). The full table stays in the file.",
    "",
    "Be exhaustive in the FILE: a short report that misses a load-bearing gap is a failure.",
  ].join("\n");
}

const WAVE = 2;
const summaries = [];
for (let i = 0; i < PAIRS.length; i += WAVE) {
  const batch = PAIRS.slice(i, i + WAVE);
  const results = await runs.all(batch.map((p) => ({
    key: p.key,
    agent: "worker",
    model: "local/local",
    task: auditTask(p),
  })));
  for (let j = 0; j < batch.length; j++) {
    summaries.push({ pair: batch[j].key, ok: results[j].ok, summary: results[j].output });
  }
}
return summaries;
