# Task 025h — the load-results layout regression (window-size follow-up)

Lane 025h. Master green (gate 4/4, 748 tests) at `8824b881`. **No worktree**
(D-035): work in the main checkout, commit every 3-5 tool calls.

## The regression (device-verified by the orchestrator)

After 025g (the menu layout now follows the REAL window size instead of the
config size), the **load-results screen is mis-laid-out on the phone**:
the count digits overlap their labels. A device screenshot shows
"PACKS **8**OADED", "LEVEL**S 37**OADED", "ASSE**TS 61**OADED",
"ER**0**RS" — the numbers are drawn at the labels' position, not beneath
them. Everything else on that screen (the hexagon, the separators, the
footer) looks correct.

## Why (the diagnosis to verify, then fix)

`draw_load_results` (`crates/oh-app/src/menu_draw.rs:1842`) positions
everything from `s.w` / `s.h` — which 025g changed from the config size to
the actual window size (now ~1530x678-ish on the phone). The label and
value positions are computed from `w`/`h`, but the **text is laid out with
font sizes / metrics that come from somewhere else** (the `char_sizes`
computed by `refresh_camera` from `four_by_three`, and/or the text-draw
helpers' own size handling). With the positions now scaled differently from
the glyph size, the value lands where the label is.

The C++ (`MenuGame.cpp:4128-4210`, the `drawLoadResults` family) computes
both from the same `w`/`h`/`charSizes` it just refreshed — so both scale
together. Verify which side is stale in the port and make them consistent.

## What to do

1. **Read the C++** `drawLoadResults` (`MenuGame.cpp:4128-4210` and the
   helpers it calls) and the port's `draw_load_results` /
   `crates/oh-app/src/menu_draw.rs` equivalents side by side. Identify
   EVERY input: positions, font sizes, the `charSizes` index used, the
   scrollbar geometry.
2. **Find what 025g broke**: compare against `git show 3fa6fcab^:`
   versions if useful (`git diff 9d6be51c..8824b881 -- crates/oh-app
   crates/oh-menu`), and in particular check whether `refresh_camera`'s
   `char_sizes`/`four_by_three` derivation still matches the C++ now that
   its inputs changed (`MenuGame.cpp:3160-3255`, the `fourByThree` and
   `charSizes` computation).
3. Fix it **1:1 with the C++** so the layout is correct at ANY window
   aspect (the phone's 1530x678 AND the desktop's config size AND the
   square-ish case). Add unit tests that pin the label and its value
   position for two different window sizes (the C++ formula, not
   eyeballed values), asserting the value sits BELOW its label in both.
4. **Device acceptance**: on the phone (adb `192.168.10.138:38721`; wake
   with `KEYCODE_WAKEUP`; `pm clear` for a fresh install), the
   load-results screen must render with each count under its label and no
   overlap — screencap evidence. THEN continue the 025g acceptance that
   could not be completed: type a name via the IME, tap the screen center,
   and the profile must be created with the main menu showing
   `CURRENT PROFILE: <name>` (touch-only submit; report the exact physical
   coordinates you tapped and why they fall in the middle third given the
   fixed window-size math).
5. **Report** (short): the stale input you found and fixed, the test names,
   the device evidence for BOTH the layout and the touch submit, and any
   residual risk. Update the deviations register row (issue 28's rule) if
   the layout issue reveals another deviation.

## Hygiene

- Commit every 3-5 tool calls (three lanes have lost work to timeouts).
- Do NOT use worktrees. Do NOT touch the emulator or non-phone processes.
- Keep the host gate green (`tools/gate.sh`, currently 748 tests).
- Screenshot histograms over image reads; at most ONE image read, at the end.
