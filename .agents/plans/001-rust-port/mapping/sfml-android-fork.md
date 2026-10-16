# SFML-fork Android investigation (issue 25 addendum)

Investigator: orchestrator, 2026-10-16. Sources: the pinned fork
`vittorioromeo/SFML@dacceddbd1adee49c7bfd120266e200800550394`
(`CMakeLists.txt:110` is the pin), read over the GitHub API.

## Question

The C++ has an Android build (`SSVOH_ANDROID`, auto-detected at
`CMakeLists.txt:26-33`), and the operator suspected the Android work was
"partly done, then hit a blocker." Was it?

## Answer: yes — Android *builds and runs*, but the game itself never wired
the mobile IME, and the platform layer lives entirely in the SFML fork.

### What the fork provides (the real Android support)

`src/SFML/Window/Android/` — a complete backend: `WindowImplAndroid.cpp`
(window/GL surface, the native-window lifecycle), `InputImpl.cpp` (touch +
**the soft keyboard**), `JoystickImpl`, `SensorImpl`, `ClipboardImpl`,
`CursorImpl`, `VideoModeImpl`. `src/SFML/Main/SFMLActivity.cpp` is the
glue: it defines `ANativeActivity_onCreate`, does the JNI dance to locate
the app's own library from `android.app.nativeLibraryDir`, `dlopen`s it
(`RTLD_NOW|RTLD_GLOBAL`), and calls the app's `ANativeActivity_onCreate`.

**Structural consequence (the "twolib" model):** the app ships as a *library*,
not an executable. The example's manifest is the proof
(`examples/android/app/src/main/AndroidManifest.xml`):

```xml
<activity android:name="android.app.NativeActivity" ...>
    <meta-data android:name="android.app.lib_name" android:value="sfml-activity-d" />
    <meta-data android:name="sfml.app.lib_name"     android:value="sfml-example" />
```

So the *activity's* library is SFML's `sfml-activity`, and SFML's glue then
loads the *game's* library (`sfml.app.lib_name`) as a second `.so`, calling
its `ANativeActivity_onCreate`. The game's `int main(argc, argv)` is
reached through `sfml-main`'s Android translation unit, not by the OS.
(SFML's own `tools/android/*.sh` + `src/SFML/Android.mk` prebuilt-module
setup is the build recipe; the scripts' own readme calls them "unpolished
at the moment" — `tools/android/readme.txt`.)

### The blocker (exactly what you suspected)

The fork exposes the mobile keyboard as **`sf::Keyboard::setVirtualKeyboardVisible(bool)`**
(`include/SFML/Window/Keyboard.hpp:412`) and implements it with **raw JNI**
in `InputImpl.cpp:80-150` — `InputMethodManager.showSoftInput(decorView, 0)` /
`hideSoftInputFromWindow(...)`, reached through `activity->env`/`activity->clazz`
(i.e. it needs an `ANativeActivity*` + JNI env, so it only works under the
SFML activity route).

**The game never calls it.** `grep -rn "setVirtualKeyboardVisible\|VirtualKeyboard"
src/ include/` → *nothing*. So in the C++ Android build, the name-entry
screens (`ETLPNewBoot` / entering-text states) have no way to raise the
keyboard from game code: the game relies on SFML's *implicit* IME behavior
(`windowSoftInputMode` / the platform showing it for the focused view) and
never asks for it explicitly.

That is a genuine partial-support edge in the C++ itself, and it is
consistent with the other Android gaps in this repo: `SSVOH_ANDROID` only
ever *excludes* desktop subsystems (CMakeLists.txt `:169,:197` ImGui +
imgui-sfml; `:311` Discord; `:418,:455,:493` the Windows/macOS/Linux
`PUBLIC_LIBRARIES` incl. steam/discord/sodium; `:507,:545,:571` pthread;
`HGUpdate.cpp:31,:1117,:1151,:1289` the ImGui Lua-console code paths;
`PCH.hpp:102`; `Discord.cpp`; `Steam.cpp`; `Imgui.cpp`). Nothing in the
game adds an Android-specific *feature* — the mobile UX all lives in SFML.

### Text input on Android (two halves, and which half the fork owns)

- **Key/character events:** `WindowImplAndroid.cpp:400-455` — `AKeyEvent`
  → `androidKeyToSF` (`:569+`, a full `AKEYCODE_*` table) for
  `KeyPressed`/`KeyReleased`, and the **`TextEntered` event is synthesized
  from the key event's unicode on `AKEY_EVENT_ACTION_UP`** (`:421-423`),
  with an `AKEY_EVENT_ACTION_MULTIPLE` arm for composing/sequences
  (`:427-455`). Notable: it uses the *key event's* unicode, not an
  `InputConnection.commitText` path.
- **Keyboard visibility:** the JNI `setVirtualKeyboardVisible` above.

Our port's 025d lane independently landed the equivalent of the first half
(`KeyboardInput.text` → `TextEntered` → `enteredChars`), and 025f lands the
second half (`show_soft_input` on the text-entry edge). So the port ends up
**functionally equivalent to the fork's behavior** — with a documented
divergence: the fork's `TextEntered` derives from key-event unicode with a
composing arm, while ours restores `KeyboardInput.text` (which is what
android-activity/winit expose). That difference is worth a mapping note but
does not change the game-visible result for ASCII names.

### Divergence vs. our implementation (deliberate, recorded)

| Aspect | C++ (SFML fork) | Port (issue 25) |
|---|---|---|
| App packaging | twolib: `sfml-activity` activity lib + the game lib loaded by JNI glue; `android.app.lib_name=sfml-activity-d`, `sfml.app.lib_name=<game>` | single lib: `android.app.NativeActivity` + `android.app.lib_name=liboh_android.so` (`android_main`, D-026 native-activity) |
| IME raising | `sf::Keyboard::setVirtualKeyboardVisible` (JNI on the decor view) — **never called by the game** | `AndroidApp::show_soft_input(true)` on the game's `isEnteringText()` edge (025f) |
| TextEntered source | key-event unicode on ACTION_UP (+ `ACTION_MULTIPLE` composing arm) | winit `KeyboardInput.text` (android-activity `TextInputState`) |
| Touch controls | game-side `updateInput_UpdateTouchControls` (ported, 25b SG5) | same (ported; `finger_down_positions` seam) |
| ImGui console | compiled out on Android | absent on Android (D-022/D-023) |
| Steam/Discord/net | compiled out / not globbed | absent (feature-off / no android arm) |

## Consequences for the port

1. **No reference implementation to copy for the IME** — confirmed, and now
   documented (D-031 already framed this; this file is the evidence). Our
   runtime-`show_soft_input` route is the correct analogue of the fork's
   `setVirtualKeyboardVisible`, and it is *better* than the C++ in one
   respect: the C++ never calls it, so the C++ Android build likely cannot
   create a profile without a hardware keyboard either. That is a
   C++-side gap our port closes; it must be recorded as a deliberate
   improvement, not a porting artifact (see the mapping note below).
2. **Manifest/activity shape is settled**: our single-lib NativeActivity
   route (D-026) is a legitimate simplification of the fork's twolib
   `sfml-activity` indirection, and it is what android-activity/winit
   support. The fork's route exists because SFML had to bootstrap the
   game's library by JNI; android-activity does that natively.
3. **`windowSoftInputMode`**: the fork's example manifest does not set it
   either (only `configChanges="keyboardHidden|orientation|screenSize"`),
   which corroborates the 025f decision (option A): the explicit show call
   is the mechanism, not the manifest attribute.
4. **Touch/`TextEntered` semantics** are worth pinning in the mapping docs
   since a future parity test on Android would compare against the fork.

## Follow-ups

- `mapping/rendering-menu.md` (or a new `mapping/input-android.md`): record
  the fork's `TextEntered` derivation and the `setVirtualKeyboardVisible`
  JNI mechanism, plus the table above, so the next reader sees the
  reference and our divergence in one place. (Worker-sized; issue 27's
  mapping-coverage guard covers it.)
- Note in `25-android.md`: the C++ Android build's keyboard gap (game code
  never raises the IME) — our port fixes it; do not "fix" the port back to
  the C++'s behavior.
- If anyone later pursues the twolib route (e.g. for `sfml.app.lib_name`
  compatibility), the fork's `SFMLActivity.cpp` is the template — but
  there is no functional reason to.
