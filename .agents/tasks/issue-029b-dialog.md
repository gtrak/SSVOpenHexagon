# Task 029b-dialog — the dialog-box input state machine (29b3 + the dialog geometry drift)

Gate green at **864 tests**. **No worktree** (D-035). One commit per item.

This is the last "the user literally cannot do this" gap in the menu: **you cannot
type a username or password**, so the online login/registration/delete flows are
unreachable. Evidence: `.agents/audits/dialog.md` (MISSING #1–#3) and
`.agents/audits/menu.md` (#3). Citations below were re-derived.

The good news: the joystick work (029j) already landed the rising-edge handlers, so
all three release paths the C++ wires (keyboard / mouse / joystick) can now be
hooked up for real.

---

## 1. 29b3 — the dialog input state machine  [uservisible-gameplay]

Three missing pieces, all in `MenuGame.cpp`, and all absent from the port:

**(a) The character echo — `MenuGame.cpp:491-517`.** A second `TextEntered`
handler: when a dialog box is open and is an input box, append
`mEvent.text.characters[0]` for the printable range (the C++ checks the unicode
value) to `dialogBox.getInput()`, capped at 32 chars, with `beep.ogg`; and pop on
**Backspace** (unicode 8) with `beep.ogg`. In the port, `input_mut()` has exactly
one call site — a default-fill at `menu_game.rs:537` — and `handle_text_entered`
(`:1934-1941`) only feeds `entered_chars` (the profile-name path). So nothing ever
reaches the dialog's input field.

**(b) The `checkCloseDialogBox` sequence machine — `MenuGame.cpp:384-489`.** On the
`keyToClose` (Enter) release: `closeBox` (`:386-391`), the gate (`:408-410`), then
per `DialogInputState`: `Nothing` (`:412-416`), `RegUsername` (`:419-427`),
`RegPassword` (`:430-438`, a masked re-dialog + `setIgnoreAllInputs(1)`),
`RegConfirm` + `tryRegister` (`:440-459`, including the **PASSWORD MISMATCH**
dialog + `error.ogg` + `setIgnoreAllInputs(1)`), `LoginUsername` (`:465-473`),
`LoginPassword` + `tryLogin` (`:476-480`), `DeleteAccount` + `tryDeleteAccount`
(`:483-487`). Dispatch sites: Key `:544-565`, mouse `:636-643`, joystick
`:693-700` — all three must be wired (the joystick seam exists now).

Port state: the `DialogInputState` values are only ever *assigned* at entry
(`menu.rs:743-761`, `menu_game.rs:516-518`) and **never transitioned**;
`handle_release_edge` (`:1992-2038`) returns early for `key_to_close != Unknown`
and takes no key; `try_register`/`try_login`/`try_delete_account` exist only as
trait declarations with no-op defaults (`seams.rs:480-483`, `:1305-1308`). So
registration/login/delete-account are unreachable.

**(c) Escape-cancel — `MenuGame.cpp:552-561`.** While an input box is open,
Escape does `setIgnoreAllInputs(0)`, `dialogInputState = Nothing`, `select.ogg`,
`clearDialogBox()` — the only way out of a started sequence other than finishing
it. Absent entirely.

**Do:** port all three faithfully — the char filter and the 32-cap, the exact
state transitions and their order, the mismatched-password path, and the Escape
cancel. Keep the network calls behind the existing seams (`try_login` etc.) and
test with a recording fake so the sequence transitions are asserted without a
server. Required tests: typing appends (and the cap holds), Backspace pops, the
password field is masked on the re-dialog, a mismatched confirm shows the error
dialog and does NOT call register, the happy registration path calls
`try_register` with the typed values, login calls `try_login`, delete-account
calls `try_delete_account`, and Escape cancels from mid-sequence.
Follow the C++ for the **exact** strings ("PASSWORD MISMATCH", the prompt texts)
— character-for-character, as with the hint strings.

## 2. The dialog-box geometry drift (D1–D6, cosmetic)

`.agents/audits/dialog.md`'s SEAMS/DIFFERENT section records cosmetic geometry
drift between the port's frame-quad math / `drawText` pass and the C++. Read those
six items and port them (or, for any that are deliberate, say so and mark it).
This rides along because it is the same file and the same reviewer context.

---

## Hygiene

- **The phone's ADB is DOWN** (`adb connect 192.168.10.138:38721` → refused; the
  host still pings it). No device work; do not add the temporary `debuggable`
  flag. The dialog is visually confirmable later — record that as **PENDING**, and
  make the host tests carry the correctness argument.
- One commit per item, naming the row id. **Stage explicit paths — never
  `git add -A`** (shared checkout + index).
- Keep the gate green after each item (`bash tools/gate.sh`, expect 864 + yours).
- Refresh `deviations.md` rows (issue 28's rule); `.agents/audits/*.md` are
  committed reports — do not rewrite them.
- If the work overruns your context, STOP after a coherent piece, commit what is
  green, and report what remains — never leave the tree mid-change.
- Append a `## Report`: the fix, the verified citations, the tests, and what you
  could not establish.
