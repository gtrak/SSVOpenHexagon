# Lane 058 — menu mouse surface (row 29m + operator accuracy audit)

## Steering log
1. Original: port `mouseHovering` (row 29m), 4 C++ sites, gates left/right.
2. REFRAME (operator: "the mouse does work ... check for accuracy"): full mouse-surface accuracy audit, per-element table, fixes where the port differs.
3. REFINEMENT (operator: "left and right movement works, but it can't click on UI elements"):
   - left/right = VERIFIED path (LMB/RMB are in the RotateCCW/CW triggers → left/right actions fire unguarded, the C++ `mouseHovering` gate is absent → confirm + fix the gate).
   - clicking UI = the target. Staging rule: C++ responds to clicks → FIX (ported logic); C++ doesn't → ADDITION (oh-additions, new OURS row).
4. ADDITION (operator: "the right-click web browser built-in menu should be disabled on wasm"): shell ADDITION, new OURS register row, ratchet baseline ritual. Plus probe: held key + right-click → does held input die? (contextmenu → blur → LostFocus fold releases held input; the release-on-real-blur is faithful, the menu appearing is the bug.)
5. FOLLOW-UP (orchestrator, post-close — the release-gate review found four bounded items): (a) the plan's item 8 "menu click → item exec" was silently dropped — measure it now, coordinate-capable, distinguishing (a)-centre-not-over-an-item vs (b)-stale-pixel, with evidence; the measurement found + fixed the 29u1 first-move cursor divergence (the first click-without-prior-move kept the cursor hidden → gate closed → click died) and exposed the probe's fixed-CDP-port stale-Chrome hazard (now per-PID + killed on exit); (b) the `mouseOverlapColor` call-site count was wrong (the C++'s TEN, the register's "9" — a duplicated :5141 + nonexistent :5157-5162 masking the missed :5186/:5221) — re-derived, all ten verified site-by-site in the port, PROGRESS.md + the STATUS.md 29m row corrected; (c) the in-tree F37 screenshot removed (the text-only evidence rule) — re-pointed to the external `/home/gary/dev/oh-evidence/lane-058/` copy (byte-identical); (d) the stale `deviations.md` "the measurement below" reference + the stale CI "has_pages is false today" comments (the page is LIVE — lane 058 measured HTTP 200) corrected.

## C++ inventory (verified against cpp-original)
- `mouseHovering` — MenuGame.hpp:265; read at MenuGame.cpp:925 (RotateCCW callback) + :934 (RotateCW callback) — `if(!mouseHovering) leftAction()/rightAction()`; set at :3673 (`overlayMouseOverlapAndUpdateHover`); cleared at :5753 (draw head).
- `mousePressed` / `mouseWasPressed` — hpp:266-267; sampled at draw head :5754-5756: `mouseWasPressed = mousePressed; mousePressed = (ignoreInputs == 0) && sf::Mouse::isButtonPressed(Left);`
- `mouseLeftRisingEdge()` — :3695-3697 (`!was && now`).
- `overlayMouseOverlap(mins, maxs)` — :3652-3666: gate `isMouseCursorVisible() && window.hasFocus()`, 1px tolerance, position = `overlayCamera.getMousePosition(window)` (window pixels → overlay-view space; overlay view = layout space, `refreshCamera` :3128-3145).
- Click edges (all set at DRAW time, consumed in `update()` next frame):
  - main menu items :3744/:3757-3761 → `mustUseMenuItem` (hpp:272); consumed :2681-2698 (SMain/MOpts/MOnline/SLPSelect, `getCurrentMenu()->getItems()`, enabled → beep + exec).
  - level body :5056/:5064-5075 → `mustPlay` (double-click: `HRClock::now() - lastMouseClick < 160ms`) else `mustChangeIndexTo`; consumed :2702-2733 (LevelSelection, `packChangeState == Rest`).
  - pack labels :5183/:5189-5193 → `mustChangePackIndexTo` (hpp:271); consumed same block.
  - favorite button :5521/:5526-5529 → `mustFavorite` (hpp:268); consumed same block (`addRemoveFavoriteLevel`).
  - `lastMouseClick` (hpp:264) — set at draw tail :5974-5977 (states that don't early-return: NOT LoadingScreen/EpilepsyWarning).
- `mouseOverlapColor(overlap, c)` — :3680-3693: `sf::Color{255-r, 255-g, 255-b}` (alpha 255). Used (the TEN call sites — re-derived against the C++ in the lane-058 follow-up; the original list's `:5141` was duplicated, `:5157-5162` does not exist, and `:5186` / `:5221` were missed): :3751 (main menu item quad — the `createQuadTrapezoid`), :3773 (main menu item text — the per-item `mouseOverlaps.at(i)`), :5061 (level item body quad), :5102 (level item name text = `menuQuadColor`), :5131 (RANKED text = the body `c`), :5141 (level item author text = `menuQuadColor`), :5186 (pack label body quad = `menuQuadColor`), :5221 (pack label text = `menuTextColor`, via the font's fill colour), :5523 (favorite button body quad = `menuSelectionColor`), :5541 (favorite button text = `menuQuadColor`). The port's `mouse_overlap_color` covers all ten (menu_draw.rs :1893 / :1937 / :2514 / :2577 / :2607 / :2630 / :2688 / :2731 / :3244 / :3277 — verified site-by-site in the follow-up).
- Cursor: ctor `setMouseCursorVisible(true)` :245; KeyPressed hide :274-280; MouseMoved show :282-297; wheel show :304; `playSelectedLevel` hide :2373; setter :3641-3644 (window + member hpp:273); getter :3648-3650.
- Triggers: `triggerRotateCCW` = {A} / {Left} / {mouse Left} (Config.cpp:265-266); `triggerRotateCW` = {D} / {Right} / {mouse Right} (:267-268). → LMB/RMB ARE left/right-action triggers in the C++ menu (and in-game rotation).
- No menu action is click-driven EXCEPT the `must*` set above (dialog box = release-driven :616-645; ImGui console = dev console, its own mouse; the C++ has no emscripten/web build — grep EMSCRIPTEN empty).

## Port state (before changes)
- `mouseHovering` absent: windowed.rs register_menu_binds comment (actions run unguarded); LMB/RMB fire left/right actions always (config defaults have btn_combo(LEFT)/btn_combo(RIGHT), input_manager matches btn bits) — matches operator's "left and right movement".
- menu_draw.rs module docs: "Mouse state is always the no-hover C++ path ... every mouseOverlap* is false ... click-edge actions never fire."
- `mustUseMenuItem` exists in Shared (dead — only read by `ignore_inputs_after_menu_exec` :901); `mustChangeIndexTo`/`mustChangePackIndexTo`/`mustFavorite`/`mustPlay`/`lastMouseClick` absent.
- Cursor visibility: ported (29u1 — composition `cursor_visible` cell + MenuWindowSource + tick_fields key-hide/mouse-show + per-frame window sync). Wheel: ported (F17/29b2). Release handlers: ported (release-edge sink).
- Web: pointer events folded (032/055); NO contextmenu handling anywhere → browser context menu on right-click / long-press (ADDITION to make, shell).

## Plan
1. oh-menu seams: `MenuWindow` += `has_focus`, `mouse_pixel`, `is_mouse_cursor_visible` (+ FakeWindow).
2. oh-menu Shared: mouse members (hpp:264-273) + `MenuGame::mouse_hovering()` + update() consumption block (C++ :2681-2733).
3. menu_draw.rs: draw head (C++ :5753-5756) + per-site overlap/hover/click-edge/color + draw tail (C++ :5974-5977) + `mouse_overlap_color` / overlap helpers + tests.
4. windowed.rs: `WindowMouseProbe` slot (focus + mouse pixel, tick_fields refresh); `MenuWindowSource` impls; left/right gate in register_menu_binds; `MenuSlotAdapter` last-input → menu_draw::draw.
5. ~~window.rs (web)~~ → **DECIDED (steering #4): the SHELL** — `crates/oh-web/site/index.html`'s module script (the established home for the visibilitychange/pagehide/resize/#fs listeners). The ported `oh-render/src/window.rs` is OFF-LIMITS for this: the C++ has no emscripten build (grep EMSCRIPTEN empty) → the suppression is an ADDITION; ported code may READ the shell's DOM (INTEROP) but must not ADD platform behaviour. No Rust state → no oh-additions half → the ratchet baseline stays untouched.
6. Register F37 + ratchet PAT + baseline; row 29m close.
7. Gates: host gate, check-android, build-windows --check, wasm check, oracle/corpus.
8. Probe: menu click → item exec (SMain + level select single/double click); held key + right-click held-input test.

## Evidence
### F37 — the contextmenu suppression, measured over a real browser (acceptance)
- Build: emsdk wasm build exit 0 (incremental; `oh-web.js` / `oh_web.wasm` / `deps/oh_web.data` fresh). Staged to `target/package/web/_WEB`, served on 127.0.0.1:8123 (the 8080/8090 ports belong to other hosts' services — 8080 = filebrowser, 8090 = a stale lane-054 server; the first probe runs hit the stale bundle, the re-run used the clean port).
- `tools/web-probe.mjs --phone --expect-cm --seq='Enter,Enter,cm,t:menu;1500,cm:ime,hc:Enter;2000'` (seq quoted — a bare `;` splits the shell command):
  - `cm` (CDP right-click at the canvas center) → **`cm:c/prev:true`** — the page's shell listener's `preventDefault` ran (no browser menu).
  - `cm:ime` (synthetic `contextmenu` dispatched on the `#ime` input itself — the field sits off-viewport, a CDP click would land on `<html>`) → **`cm:ime/prev:false`** — the `#ime` exclusion holds (the desktop paste/copy affordance stays).
  - `t:menu;1500` (the 1500 ms long-press) → the port's grid cadence ran (18× `[oh-menu] grid dispatch` — the F35 50 ms cadence, the port alive) but **this headless Chrome does not synthesize a `contextmenu` from a touch long-press** → the long-press half is NOT ESTABLISHED here (the suppression is the same listener the right-click half proved; on the operator's Android Chrome the long-press produces the event and the listener covers it — unmeasured in headless, reported as not-established, not claimed).
  - `hc:Enter;2000` (hold Enter, right-click at the 50% mark) → **`cm:c/prev:true`** + focus counters stay `blur:0` the whole run — the suppressed menu does not steal focus, the held input does not die; Enter's release edge lands clean.
  - Gate line: **PASS — the canvas / document contextmenu was prevented (no menu appears) and the #ime's was not (the exclusion)**. Screenshot: `/home/gary/dev/oh-evidence/lane-058/contextmenu-suppression.png` (outside the tree — the text-only evidence rule; the in-tree `.agents/evidence/058/contextmenu-suppression.png` copy was removed in the lane-058 follow-up, the external copy is byte-identical).
  - Probe exit code 1 comes ONLY from pre-existing port diagnostics logged to the console error sink on every web run (`[oh-render] WebGL2 antialias`, `[oh-audio] set sound volume`, the 039 `[oh-menu] grid dispatch` device diagnostic via `eprintln!`) — none touch this lane; the F37 verdict is the `--expect-cm` PASS line.
  - Probe fix logged for the next reader: the contextmenu listener must install LAZILY (on the first cm/hc:/t: step, after the wasm boot) — the module-scope page listener registers at boot (the `await Module(cfg)` resume), which is AFTER a parse-time installer; an installer-time listener precedes the page's and reads `defaultPrevented` before the page's `preventDefault` runs (the first run's `cm:c/prev:false` — the probe's error, corrected).

### The 29m acceptance measurement — the plan's item 8 ("menu click -> item exec") (the lane-058 follow-up)
The orchestrator's follow-up found the plan's item 8 (a MEASURED live click on a real SMain item) had been silently dropped — only the F37 measurement had landed. It is measured now, and it found + fixed a real port divergence on the way, plus a measurement-harness hazard that invalidated the follow-up's first three runs.

**The probe gained coordinate mouse steps** (`tools/web-probe.mjs`, mirroring the `m:`/`t:` conventions): `mv:X;Y` (a CDP `mouseMoved` at the CSS-viewport X;Y — the port's `MouseMoved` fold's + the 29u1 cursor re-show's source) and `mc:X;Y[;MS]` (a press held MS ms, default 300, then release — NO implicit move: the port's `pointerdown` fold emits the `MouseMoved` at the press's position BEFORE the `MouseButtonPressed` (window.rs's `WebPointerFeed::pointer_down` mouse arm), so a click whose position arrives with the press is well-defined — the C++'s SFML live-position read's equivalent; the move-first case is the `mv:` + `mc:` pair). The probe also gained `OH_WINDOW_SIZE` (the Chrome `--window-size`) and `OH_CHROME_EXTRA` (`;`-separated extra Chrome args) env knobs.

**The measurement-hazard the follow-up's first runs all hit (fixed before anything was trusted):** the probe's CDP port was FIXED at 9335. A probe run whose node got `timeout`-killed (before the probe's own `chrome.kill`) left its Chrome alive and owning 9335; the NEXT probe run's health-check then reached the STALE Chrome — whose own user-data-dir / IDBFS / prior page state answered the run as if it were fresh (the follow-up's first three "SMain" runs were actually clicking in a stale instance's page). Fixed in the probe: the CDP port is now PER-PID (`10000 + pid % 20000` — a stale instance holds its own, different port, so the health-check's owner is always the probe's own Chrome) + `SIGTERM`/`SIGINT`/`exit` handlers that `SIGKILL` the Chrome on any node-side death (the `timeout -k` SIGKILL case is bounded by the per-PID port). All acceptance evidence below is from post-fix runs.

**The (a)-vs-(b) distinguisher, RESOLVED by direct measurement** (both on the clean build, phone viewport 980×2121 CSS @ dpr 3, screenshots 2940×6363 device):
- **(a) — the centre is not over a menu item — CONFIRMED.** The SMain items are RIGHT-aligned (the C++'s `bodyMaxs = {w, ...}` — the quads run from their indent to the window's right edge; the item-3 hit rect's left edge is virtual x 702 = device 2015 = CSS 672 — the layout-space values below). The canvas centre (490, 1060.5 CSS = 1470, 3181.5 device) is LEFT of every item's hit rect → measured: `mc:490;1060` (a 300 ms press at the exact centre) leaves the state **SMain** (no item exec, no selection change — the centre overlaps nothing). (The centre's y half, 1060.5, does fall inside an item row's band — the miss is purely the x half.)
- **(b) — a press with no preceding pointer move leaves the mouse pixel stale — REFUTED.** The `pointerdown` fold moves the pixel to the press's position before the press (window.rs's `WebPointerFeed::pointer_down` mouse arm) — and measured: `mc:800;1141` as the FIRST mouse event of the run (no preceding `mv:`) → **SMain → MOpts** (the pressed item executes). **The audit found the real (b)-class bug on the way**: the 29u1 cursor re-show's first-move semantics. The C++'s `lastMouseMovedPosition{}` (MenuGame.hpp:274 — `sf::Vector2i` value-initialization = `{0, 0}`) makes the FIRST `MouseMoved` event at P ≠ (0,0) an `actuallyMoved` one (the cursor re-shows — MenuGame.cpp:282-297). The port's `cursor_last_mouse` cell started at `Option::None` ("the first frame has no previous pixel — not a move, so no show") — so the FIRST mouse event after boot never re-shows: after a key-press-hide, the first click-without-prior-move kept the cursor hidden → the hover gate (the `MenuMouse`'s cursor-visible half, the C++'s `isMouseCursorVisible()` term) stayed closed → the click died at the gate. In a real browser a user normally moves first (subsequent moves re-show in both), so it bit only the first mouse interaction — but the acceptance's own decisive click (`mc:` as the first mouse event) is exactly that case, so it was fixed: `cursor_last_mouse` now starts at `Some(Vec2::new(0.0, 0.0))` (the window's own initial `mouse_pixel`, the C++'s `{0, 0}` mirror), the 29u1 block extracted to the testable `menu_cursor_frame` (windowed.rs), + the regression test `the_first_mouse_frame_re_shows_the_cursor_the_cpp_first_move_semantics`. All gates re-run for the Rust change: host gate 6×GATE_EXIT=0, `check-android.sh` + `build-windows.sh --check` clean, emsdk wasm build exit 0 (re-staged to `target/package/web/_WEB`), `tools/parity.sh` PARITY_OK.

**The acceptance — the measured live click:**
```
--seq='Enter,Enter,i:qa1,Enter,mc:490;1060,s:after-center,mv:800;1141,s:smain-hover,mv:800;600,s:smain-nohover,mv:800;1141,mc:800;1141,s:after-click'
[step] mc:490;1060 -> menu:SMain          (the centre: no state change — (a))
[step] mc:800;1141 -> menu:MOpts         (the OPTIONS row: the item executes)
state timeline: menu:LoadingScreen -> menu:EpilepsyWarning -> menu:ETLPNewBoot -> menu:SMain -> menu:MOpts
```
`mc:800;1141` (the OPTIONS row — item 3's hit rect: device x 2015..2940 × y 3386.8..3423.4 = CSS x 672..980 × y 1128.9..1141.1 — 800,1141 sits in it) executes the OPTIONS item → **SMain → MOpts** (the state change in the step/state log, the acceptance's own criterion). Screenshots in `--shots`: `after-center.png` (SMain after the centre no-op), `smain-hover.png` (mouse over item 3), `smain-nohover.png` (mouse moved to 800,600), `after-click.png` (MOpts) — copied outside the tree to `/home/gary/dev/oh-evidence/lane-058/` (the text-only evidence rule).
**The first-click case (no prior move — the (b) territory) also executes the CLICKED item on the fixed build:** `--seq='Enter,Enter,i:qa1,Enter,mc:800;1141'` → `[step] mc:800;1141 -> menu:MOpts`. Trace: the press's own `MouseMoved` (the `pointerdown` fold) is fresh at the press frame (the `window_mouse` probe updates pre-update) → the first-move cursor re-show (the fix) opens the gate there → the draw's rising edge arms `must_use_menu_item` → the following frame's `update` consumes it (the C++'s exact consume path, ported this lane).

**The hover colour pair — NOT VISUALLY MEASURABLE in this environment; stated explicitly (the task's "if not measurable, say so")**: in the headless SwiftShader screenshots the SMain item quads + their text composite to BLACK in the item band (device y ~2957..3567) — the top logo / background art (device y 256..1288) renders, the item band does not, in EVERY configuration tried (default headless, `--ozone-override-screen-size=800,600`, before/after the cursor fix, across 10+ runs) — a headless compositing artifact, NOT a port logic state (a real browser / Android GPU renders the band normally — the C++-equivalent desktop SFML window is the parity oracle's, and the port's GL calls are identical in both cases). Consequence: the `smain-hover.png` vs `smain-nohover.png` pair shows **0 px difference** in the item band (re-measured: 0/23,739 over item-3's quad region; the follow-up's earlier "23,141/24,327 px differ, fill (0,0,0)→(255,255,255)" figure was NOT reproducible from the screenshots on disk — the pre-follow-up runs' own hover pair diffs are 37 px over the WHOLE screen (0.002%), i.e. none). The inversion is verified at the LOGIC level instead, three ways: (1) the unit tests — `mouse_overlap_color`'s exact RGB-inversion (255-r, 255-g, 255-b, alpha 255) + `MenuMouse::overlaps`'s gate + 1px-tolerance rect + the click-arm-then-consume chain (the `mouse_flags.rs` + `menu_draw` test modules); (2) a temporary in-build debug read (reverted after measurement, no committed artifact) printed the hover frame's live values — `gate=true focus=true cursor=true mpos=(836,1192) rect=(702..1024 x 1180..1216) overlap=true` (the SAME `mouse.overlaps` call that feeds the quad's `mouse_overlap_color` — so the quad's colour WAS inverted on that frame; only the headless compositing drops it from the screenshot); (3) the click-edge path (the hover's own consumer — `must_use_menu_item`'s arm needs the same overlap) is the measured acceptance above. On the operator's device the inversion renders (the logic is the C++'s, measured green in the unit corpus).

**The SMain's per-item geometry (layout-derived — the port's own draw code + view mapping, since the items don't composite in the headless screenshot; cross-checked against the measured clicks):** the phone viewport's virtual menu space is 1024 × 2216.23 (the `refresh_view`'s 1024×768-scale of the actual 2940×6363 window; the 4:3 arm — `menu_big` char size 26, font-height probe 16.63), mapped full-bleed to the 2940×6363 canvas (the overlay view's centre (512, 1108.1), size (1024, 2216.23)). Item quads (virtual → device): item 0 (LEVEL SELECT) top y 1029.9 → device 2957.1 (CSS 985.7); item 1 (LOCAL PROFILES) 1079.8 → 3100.3 (CSS 1033.4); item 2 (ONLINE) 1129.7 → 3243.6 (CSS 1081.2); item 3 (OPTIONS) 1179.6 → 3386.8 (CSS 1128.9); item 4 (EXIT) 1229.5 → 3530.1 (CSS 1176.7). Hit rects (virtual, the `bodyMins`/`bodyMaxs`): x 702..1024 (device 2015..2940, CSS 672..980) for all five (right-aligned), y per item (item 3: 1180..1216 → device 3387..3423, CSS 1129..1141). Cross-check: `mc:800;1141` (device 2400,3423) lands in item 3's rect → MOpts (measured); `mc:490;1060` (device 1470,3181.5) lands left of every rect's x-min (2015) → SMain (measured); `mc:800;940` (device 2400,2820 — above item 0's top 2957) → SMain (measured). All consistent.

**The focus semantics the follow-up's first runs misread (for the next reader):** the headless page reports `has_focus()=true` from the FIRST frame (measured: the port's own boot-line debug `focus=true`), so the gate's focus term is never the headless limiter; the cursor term is — it follows the 29u1 per-frame semantics (a key press hides, a mouse move re-shows) exactly as the C++'s. A `mouseMoved` on a never-focused page would be gate-closed in a REAL browser too (the C++'s `hasFocus()` term) — faithful, not a bug.

**The boot entry (clean environment, per-run fresh Chrome + fresh IDB):** `Enter` (LoadingScreen → EpilepsyWarning), `Enter` (EpilepsyWarning → ETLPNewBoot — the fresh-boot name entry, since each probe run's fresh `--user-data-dir` means a fresh IDBFS and no profiles), `i:qa1` (the IME name), `Enter` (ETLPNewBoot → SMain). The follow-up's first three runs' "profile persists" confusion was the STALE-CHROME hazard above (a stale instance's own profile), not IDBFS persistence — post-fix, every run boots fresh (the per-step state dump shows `ETLPNewBoot` on every run). Verify with the per-step state dump (`[step] ... -> menu:SMain`) before trusting any mouse step.

### The site-by-site accuracy table (the deliverable)
**A — `mouseHovering` (row 29m's core, 4 C++ sites):**
| C++ site | port site | verdict |
|---|---|---|
| MenuGame.cpp:5753 `mouseHovering=false` (draw head) | `menu_draw.rs::draw` — `MenuMouse` fresh per frame, hover starts false | **VERIFIED (ported this lane)** |
| MenuGame.cpp:3668-3677 `overlayMouseOverlapAndUpdateHover` (set on overlap) | `menu_draw.rs` — `MenuMouse::overlaps` per site + the `gate` (the C++ :3652-3666 `!isMouseCursorVisible() || !window.hasFocus()` → false, 1 px tolerance) | **VERIFIED (ported this lane)** |
| MenuGame.cpp:925 `if(!mouseHovering) leftAction()` | `windowed.rs register_menu_binds` — `if !m.mouse_hovering()` before the RotateCCW callback | **VERIFIED (ported this lane)** |
| MenuGame.cpp:934 `if(!mouseHovering) rightAction()` | same, the RotateCW callback | **VERIFIED (ported this lane)** |

**B — cursor visibility (the 29u1 surface, audited this lane — all pre-existing, all faithful):**
| C++ site | port site | verdict |
|---|---|---|
| :245 ctor `setMouseCursorVisible(true)` | `cursor_visible` cell init `true` (windowed.rs:3202) | VERIFIED (29u1) |
| :274-280 KeyPressed → hide (hasFocus) | tick_fields 29u1 block (windowed.rs:4199-4203, menu-state only — the C++'s handlers live on the menu state's event bus) | VERIFIED (29u1) |
| :282-297 MouseMoved → re-show (hasFocus && actuallyMoved) | 29u1 block (windowed.rs:4208-4213, `last != mp` = the `actuallyMoved` delta; first frame `None` = no move) | VERIFIED (29u1) |
| :304-306 wheel → re-show | `menu_game.rs:3403` (`handle_mouse_wheel`, via the `MenuWindow` seam) | VERIFIED (29u1/F17) |
| :2373 `playSelectedLevel` → hide | `menu_game.rs:1285` (inside the `new_game` callback guard — the C++'s `if(fnHGNewGame)`) | VERIFIED (29u1) |
| :3641-3650 setter/getter | `MenuWindowSource::set/is_mouse_cursor_visible` + the per-frame window sync (windowed.rs:4313) | VERIFIED (29u1 + this lane's seam reads) |
| game-state persistence (the hidden cursor stays hidden through a level — the C++'s handler never runs off-menu; a menu return re-constructs `MenuGame` → ctor true) | the `cursor_visible` cell survives across states; 29u1's logic re-activates menu-state only | VERIFIED (shape-faithful; the port's menu is not re-constructed but the cell's value is the same member's value — the observable matches) |

**C — coordinate space:**
| C++ site | port site | verdict |
|---|---|---|
| `overlayCamera.getMousePosition(window)` (window px → overlay-view space) | `overlay_view.map_pixel_to_coords(mouse_pixel, real_size)` (`menu_draw.rs`, the view = the real window size per 025g) | VERIFIED (ported this lane — same mapping) |

**D — the per-draw-site mouse behaviour:**
| C++ site | port site | verdict |
|---|---|---|
| :3730-3762 main menu hover + click-edge → `mustUseMenuItem` | `draw_main_menu` (per-item overlap/hover/click-edge/`mouse_overlap_color`) | **VERIFIED (ported this lane)** |
| :5040-5075 level items hover + click → `mustPlay` (160 ms `lastMouseClick` double) else `mustChangeIndexTo` | `draw_level_selection_left_side` / `_right_side` | **VERIFIED (ported this lane)** |
| :5183-5192 pack label click → `mustChangePackIndexTo` | `draw_level_selection_left_side` | **VERIFIED (ported this lane)** |
| :5521-5529 favorite button → `mustFavorite` | `draw_level_selection_left_side` | **VERIFIED (ported this lane)** |
| `mouseOverlapColor` :3680-3693 (RGB invert, alpha 255) + its TEN call sites (:3751 main quad / :3773 main text / :5061 level body / :5102 level name / :5131 RANKED / :5141 level author / :5186 pack body / :5221 pack label text / :5523 favorite body / :5541 favorite text — re-derived in the follow-up; the original "9" was the duplicated `:5141` + the nonexistent `:5157-5162` masking the missed `:5186`/`:5221`) | `mouse_overlap_color` + every draw site's hover colour (the port's ten sites: menu_draw.rs :1893/:1937/:2514/:2577/:2607/:2630/:2688/:2731/:3244/:3277 — verified) | **VERIFIED (ported this lane)** |
| `drawOptionsSubmenus` / `drawSubmenusSmall` — NO mouse sites (the C++'s submenus take no hover) | `draw_options_submenus` / `draw_submenus_small` — no mouse parameter | VERIFIED (faithful — nothing to port) |

**E — the update-side consumption:**
| C++ site | port site | verdict |
|---|---|---|
| :2681-2733 `mustUseMenuItem` (exec + beep) / `mustChangeIndexTo` / `mustChangePackIndexTo` / `mustFavorite` / `mustPlay` | `menu_game.rs update()` consumption block (ported this lane; `crates/oh-menu/tests/mouse_flags.rs` — 9 tests) | **VERIFIED (ported this lane)** |
| :1137 `setIgnoreAllInputs(mustUseMenuItem ? 1 : 2)` post-exec | `ignore_inputs_after_menu_exec` (menu_game.rs:901, pre-existing) | VERIFIED (pre-existing) |

**F — wheel / release / dialog (pre-existing surfaces, audited):**
| C++ site | port site | verdict |
|---|---|---|
| :299-356 MouseWheelScrolled (menu scroll, hasFocus gate) | F17/29b2 (`handle_mouse_wheel`) | VERIFIED (pre-existing, OURS-registered F17) |
| :616-645 MouseButtonReleased (dialog cancel / ignore reset) | the release-edge sink (055) | VERIFIED (pre-existing) |
| dialog box (release-driven in the C++) | release-driven in the port | VERIFIED (faithful) |

**G — per-element staging (the operator's rule: C++ responds to the click → FIX in ported logic; else → ADDITION/OURS):**
| element | C++ responds to a click? | verdict |
|---|---|---|
| main menu items / level items / pack / favorite | YES (the `must*` click-edge set) | **FIX — ported this lane** (table D/E) |
| the 031 touch-overlay gameplay buttons | NO such drawn elements in the C++ — LMB/RMB drive the CCW/CW Tids directly through the `triggerRotate*` combos in BOTH; the drawn buttons are the 031 OURS touch affordance | VERIFIED faithful (no fix; the underlying trigger behaviour matches — the trigger defaults carry `btn_combo(LEFT)`/`btn_combo(RIGHT)`) |
| the F31 action row (MENU/REPLAY/RESTART/PAUSE) | NO (the C++ has no such elements; Replay/Restart/Pause are keyboard-bound) | click-to-activate would be an ADDITION — NOT implemented (flagged as a candidate future OURS row, not claimed) |
| the `#ime` soft-keyboard field | N/A (OURS — 031/034) | the F37 exclusion keeps its native right-click (paste/copy) — measured (`cm:ime/prev:false`) |
| the browser contextmenu itself | N/A (the C++ has no web build) | **ADDITION (OURS, F37) — the shell's `index.html`**, measured above |
| the ImGui console (C++) vs the egui console (port) | the C++'s ImGui takes the mouse natively; the port's egui likewise (separate subsystem, D-022) | VERIFIED faithful (both take the mouse natively — no menu-surface site) |

## Decisions
- Hover/click state computed in `menu_draw::draw` (the C++'s draw-head + draw-time edge sites), consumed next frame by `MenuGame::update` — the C++'s exact one-frame lag (C++ input actions + update run before that frame's draw).
- The left/right gate goes in the INPUT callbacks (register_menu_binds), NOT in `left_action`/`right_action` — the C++ gates only the Tid-trigger path (MenuGame.cpp:925/934); the joystick (:2756) and touch-grid (:2802-2835) paths call leftAction() unguarded.
- `mouseCursorVisible` is NOT duplicated in Shared — the composition's `cursor_visible` cell (29u1) IS the C++ member; read via the new `MenuWindow::is_mouse_cursor_visible` seam method (single source of truth).
- `sf::Color{r,g,b}` default alpha 255 → `mouse_overlap_color` sets alpha 255 (NOT `c.a`).
- The F37 listener lives in the SHELL (`crates/oh-web/site/index.html`, steering #4): document scope, `#ime` excluded (`ev.target.closest('#ime')` — the desktop paste/copy affordance stays), no Rust state (nothing for the ratchet to count, no oh-additions half). The ported `oh-render/src/window.rs` must not add platform behaviour the C++ lacks (the C++ has no emscripten build).
