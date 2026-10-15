# lib-gameplay — SSVOpenHexagon gameplay types (walls, player, status, timelines)

Source of truth: the C++ gameplay components under
`include/SSVOpenHexagon/{Components,Core,Data,Utils}/**` +
`src/SSVOpenHexagon/{Components,Core,Utils}/**`. The authoritative verbatim
transcription is `.agents/plans/001-rust-port/mapping/gameplay.md` (17 sections);
this skill distils the **behavior-affecting** parts a worker must not re-derive
wrong. Oracle-pinned values: `.agents/plans/001-rust-port/mapping/
oracle-vectors.md` ("Gameplay geometry") + `crates/oh-game/tests/*.rs` (g++
-std=c++20 probe of the real `CWall.cpp`/`CPlayer.cpp`/`Geometry.cpp`/`Ticker.cpp`).

Rust port: `crates/oh-game` (one module per C++ file): `speed_data`, `wall`,
`player`, `custom_wall`, `custom_wall_manager`, `status`, `level_status`,
`ticker`, `timeline`, `custom_timeline`, `vertex_batch`, `polygon`, `clock`.
Depends on `oh-math` (`Vec2`, `Color`, FT, geometry/easing/move-towards) and
`oh-rng` (used later by `HexagonGame`, not by these data types directly).

Fidelity rule (DECISIONS.md D-000): every C++ `float` → `f32`, `double` → `f64`;
constants copied verbatim; **no floating-point expression is reordered**. This
crate is where replay parity is won or lost — a reordered `a*b*c` or a swapped
sin/cos changes the last ulp and desyncs the RNG stream.

## World space & units (§16)

- Origin-centered: `centerPos = (0,0)`. The player's `_pos` is in **world**
  coords and `_angle = atan2(y, x)` is measured from the world origin, NOT from
  `centerPos` (which is the origin anyway).
- 60 FT-units = 1 s. Fixed sim tick is `TIME_STEP = 60.f/240.f = 0.25f` (240 Hz).
- Time bookkeeping is `double` (f64); all gameplay quantities are `float` (f32).
- `status.radius` is the player's orbit/collision radius. Wall collision uses
  `radiusSquared = radius*radius + 8.f` — the `+8.f` is added to the **squared**
  value, inside the same f32 expression as the square.

## SpeedData (§1) — `Components/SpeedData.hpp`

```cpp
_speed += _accel * ft;
if (_speed > _max)      { _speed = _max; _accel *= _pingPong; }
else if (_speed < _min) { _speed = _min; _accel *= _pingPong; }
```
`_pingPong` = `-1.f` if the ctor `pingPong` flag else `1.f`. `accel==0` → no-op.
Order of checks: `> _max` first, then `else if < _min`. `pingPong=false` at a
bound just re-clamps each frame (accel `*= 1.f`).

## CWall (§2) — vertex ordering & movement

Ctor (f32, exact order):
```cpp
div   = tau / (float)sides * 0.5f;          // (tau/sides)*0.5, left-assoc
angle = div * 2.f * (float)side;
angleN = angle - div;  angleP = angle + div;
v[0] = center + dist_vec(angleN, distance);
v[1] = center + dist_vec(angleP, distance);
v[2] = center + dist_vec(angleP + wallAngleLeft,  distance + thickness + wallSkewLeft);
v[3] = center + dist_vec(angleN + wallAngleRight, distance + thickness + wallSkewRight);
```
where `dist_vec(rad, d) = (d*cos(rad), d*sin(rad))` (== `getOrbitRad`).

**Vertex order is nw=0, sw=1, se=2, ne=3.** `wallSkewLeft` → v[2] (the "P" side);
`wallSkewRight` → v[3] (the "N" side). Inner edge (v0,v1) is at `distance`; outer
edge (v2,v3) at `distance + thickness (+skew)`.

`moveTowardsCenter` (bounds logic, exact):
```cpp
halfRadius = radius*0.5f;  outerBounds = wallSpawnDist*1.1f;
for each vertex vp:
  xDist = |vp.x - cx|;  yDist = |vp.y - cy|;
  if (xDist < halfRadius && yDist < halfRadius) { onCenter++; continue; }  // NOT moved
  if (xDist > outerBounds || yDist > outerBounds) outOfBounds++;
  vp += normalized(cx - vp) * _speed._speed * 5.f * ft;   // scale order preserved
killed = (onCenter==4 || outOfBounds==4);
```
AABB-ish test (x and y compared independently, `&&` for onCenter / `||` for
outOfBounds). `normalized()` leaves a zero vector unchanged (SFML guard). The
`* _speed._speed * 5.f * ft` must be applied **left-to-right** (f32 mult not
associative) — ported as `((v*speed)*5.0f32)*ft`.

`update`: `_speed.update(ft); _curve.update(ft); moveTowardsCenter(...); if
(_curve._speed != 0.f) moveCurve(...);`.

Curve rotation (§2 + §16.5/§16): `getCurveRadians(ft) = _curve._speed *
(1.f/60.f) * ft` (per-frame incremental, **not cumulative** — recomputed from the
current `_curve._speed` each frame). `moveVertexAlongCurveImpl` rotates a vertex
about `centerPos`:
```cpp
x' = tempX*xCos - tempY*xSin + cx;   // tempX = x - cx
y' = tempX*xSin + tempY*xCos + cy;   // tempY = y - cy
```
**Sign convention: positive `_curve._speed` rotates counter-clockwise in math
coords** (same as `getOrbitRad`/`getVecFromRad`). `moveCurve` recomputes
`rad/radSin/radCos` once and applies to all 4 vertices.

`isOverlapping` uses the four-vertex cross-product test (needs consistent
winding — the nw/sw/se/ne order guarantees it). `CWall::isCustomWall()==false`,
`getKillingSide()==0`.

## CPlayer (§3) — collision escape & push

Fields (ctor defaults): `_pos/_prePushPos/_lastPos/_startPos = pos`; `_angle/
_lastAngle/_hue/_currTiltedAngle = 0`; `_radius/_maxSafeDistance/_currentSpeed =
0`; `_triangleWidth = unfocusedTriangleWidth (3.f)`; `_triangleWidthTransitionTime
= 0`; `_swapTimer = Ticker{swapCooldown}`; `_swapBlinkTimer = Ticker{6.f}`;
`_deadEffectTimer = Ticker{80.f, false}` (starts NOT running); `_justSwapped/
_forcedMove/_dead = false`.

File constants: `baseThickness=5.f`, `unfocusedTriangleWidth=3.f`,
`focusedTriangleWidth=-1.5f`, `triangleWidthRange = unfocused - focused = 4.5f`
(named constants only feed the range), `collisionPadding=0.5f`.

`updatePosition(radius)`:
```cpp
_radius = radius;
_prePushPos = _pos = getOrbitRad(_startPos, _angle, _radius);
_lastPos    = getOrbitRad(_startPos, _lastAngle, _radius);
_maxSafeDistance = magSquared(_lastPos - getOrbitRad(_startPos,
                       _lastAngle + toRad(_currentSpeed), _radius)) + 32.f;
```
(`_prePushPos` is the pre-movement position, re-set every frame; it is the
reference direction for push displacement.)

`checkWallCollisionEscape` (the C++ `template<Wall>` — ported generic over a
`Wall` trait; **replay-critical**):
```cpp
saved=false;  safeDistance=_maxSafeDistance;
vxIncrement = wall.isCustomWall() ? 1 : 2;   // 2 for normal, 1 for custom
killingSide = wall.getKillingSide();
for (unsigned i=0, j=3; i<4; i += vxIncrement, j = i-1)   // j is unsigned
  if (j == killingSide) continue;
  switch (getLineCircleIntersection(vec1, vec2, v[i], v[j], r2)) {
    case 1: assign(vec1); break;
    case 2: if (magSq(vec1-pos) > magSq(vec2-pos)) vec1 = vec2; assign(vec1); break;
    default: break;
  }
// assign: d = magSq(vec1-pos); if (d < safeDistance) { pos = vec1; saved = true; safeDistance = d; }
```
Loop visitation: step-2 (normal) → `(0,3),(2,1)`; step-1 (custom) →
`(0,3),(1,0),(2,1),(3,2)`. The circle is centered at the **world origin** with
squared radius `r2`; `getLineCircleIntersection` is NOT segment-clamped (no
t∈[0,1] check). `vec1`/`vec2` persist across iterations (only written on ≥1 hit).

`push` (CWall): returns `true` = killed.
```cpp
if (_dead) return false;
testPos = _pos;  pushVel = 0;
if (curve._speed != 0 && getSign(curve._speed) != movementDir) {   // moving against rotation
    wall.moveVertexAlongCurve(testPos, center, ft);  pushVel = testPos - _pos;
}
if (!movementDir && !_forcedMove) {           // static:
    _pos = testPos + normalized(testPos - _prePushPos) * (2.f*collisionPadding); // *1.0
    _angle = atan2(_pos); updatePosition(radius); return wall.isOverlapping(_pos);
}
testPos = _lastPos + pushVel;                 // moving: escape from PREVIOUS-frame pos
if (wall.isOverlapping(testPos) || !checkWallCollisionEscape(wall, testPos, r2)) return true;
_pos = testPos + normalized(testPos - _prePushPos) * collisionPadding;   // *0.5
_angle = atan2(_pos); updatePosition(radius); return false;
```
Notes: `getSign` is `(0<x)-(x<0)` → -1/0/+1, compared to `int movementDir`
(-1/0/+1). Static displacement uses `2.f*collisionPadding == 1.0`; moving
displacement uses `collisionPadding == 0.5f`. `pushVel` only when the wall's
rotation **opposes** the movement direction.

`push` (CCustomWall): different first phase — it looks for a *sweep* side that
pushes the player like a curving wall:
```cpp
for (unsigned i=0, j=3; i<4; j = i++)        // j=old i then i++: (0,3),(1,0),(2,1),(3,2)
  if (j == killingSide) continue;
  poly = {v[i], oldV[i], oldV[j], v[j]};
  if (pointInPolygon<4>(poly, _lastPos.x, _lastPos.y)) {
     if (lineCircleClosest(i1, _lastPos, oldV[i], oldV[j], r2) &&
         lineCircleClosest(i2, _lastPos, v[i],     v[j],     r2)) {
        pushVel = i2 - i1;
        if (abs(dot(normalized(pushVel), normalized(_lastPos))) > 0.15f) pushVel = 0;
     }
     break;
  }
```
(Uses `pointInPolygon<4>` because custom-wall vertices are Lua-defined and can be
in **any** order — not the cross-product test.) Then the same static/moving
tail as the CWall push (static: `_pos += pushVel` then `*2.f*padding`; moving:
`testPos = _lastPos + pushVel`). The custom-wall loop step is `j = i++` (post-
increment): `j` takes the old `i`, then `i` increments.

`update(focused, swapEnabled, ft)`: triangle-width transition; then if
`_deadEffectTimer` running → update it, `_hue += 18.f*ft` (wrap `>360.f → 0`),
if `_dead` return; if `_deadEffectTimer.getTotal() >= 100` → stop + resetAll.
Then `_swapBlinkTimer.update(ft/3.f)`; `if (swapEnabled && _swapTimer.update(ft))
_swapTimer.stop();`; finally `_lastAngle = _angle; _forcedMove = false;`.

`updateInputMovement(movementDir, playerSpeedMult, focused, ft)`
(`movementDir` is a `float` holding -1/0/+1):
```cpp
_currentSpeed = playerSpeedMult * (focused ? _focusSpeed : _speed) * ft;  // deg/FT
_angle += toRad(_currentSpeed * movementDir);
inc = ft/10.f;
_currTiltedAngle = (movementDir==0.f) ? moveTowardsZero(_currTiltedAngle, inc)
                                      : moveTowards(_currTiltedAngle, movementDir, inc*2.f);
```

`updateTriangleWidthTransition(focused, ft)` (port EXACTLY, do NOT "fix"):
```cpp
if (focused && t<1.f) moveTowards(t, 1.f, ft*0.1f);
else if (!focused && t>0.f) moveTowardsZero(t, ft*0.1f);
_triangleWidth = triangleWidthRange * (1.f - smoothStep(0.f, 1.f, t));
```
(so t=0 → width 4.5, t=1 → width 0; the named 3.f/-1.5.f constants are not the
endpoints of this formula — the C++ is written this way.)

`kill(fatal)`: `_deadEffectTimer.restart()`; if fatal → `_dead=true`, and (only
if `!_justSwapped` && `euclidDist(_pos,_lastPos) < 24.f`) `_pos =
getOrbitRad(_lastPos, _angle, -_size)` (roll back to the triangle tip).

`playerSwap`: `_angle += pi`. `setPlayerAngle(a)`: `_angle=a; _forcedMove=true`.
`isReadyToSwap = !_swapTimer.isRunning()`. `hasChangedAngle = _angle != _lastAngle`.
`getColorAdjustedForSwap`: if `!_swapTimer.isRunning() && !_dead` →
`getColorFromHue(fmod(_swapBlinkTimer.getCurrent()/12.f, 0.2f))` else
`getColor`. `getColor`: if `!_deadEffectTimer.isRunning()` → `colorPlayer` else
`getColorFromHue(_hue/360.f)`.

Triangle draw: apex at `getOrbitRad(_pos, tiltedAngle, _size)`; base at
`±toRad(100.f)` radius `_size + _triangleWidth`; `tiltedAngle = _angle +
_currTiltedAngle*toRad(24.f)*angleTiltIntensity`. Pivot: `pRadius = _radius*0.75f`.

## CCustomWall / CCustomWallManager (§4)

`CCustomWall`: 4 `vertexPositions` + 4 `oldVertexPositions` (previous frame) + 4
`vertexColors` + `killingSide` + a 2-flag bitfield (`NoCollision`, `Deadly`).
Default = collides, not deadly. `setVertexPos(i,pos)` uses `std::exchange`
(→ old captured into `_oldVertexPositions[i]`). `isOverlapping` = even-odd
`pointInPolygon<4>`.

`CCustomWallManager` (handles = `int`, a free-list, NOT plain indices):
`create()` reserves a block when the free-list is empty:
`reserveSize = 32 + nextFree*2`, grows `customWalls`/`handleAvailable` to
`nextFree + reserveSize`, fills the free-list with `[nextFree .. nextFree+reserve)`
in ascending order, then hands out the **back** (`pop_back`) — so the first live
handle is the highest index (31 for the first block). `destroy` pushes back onto
the free-list. `count()` / `maxHandles()`.

`handleCollision(movement, radius, player, ft)` — 3 phases (re-read `player`
position fresh each check, since pushes move it):
1. For each alive+collidable wall overlapping the player: kill if
   `getJustSwapped() || wall.getDeadly() || push(...)`. (Just-swapped
   immunity lasts only this frame; `push` not called in that case.)
2. Recheck all, `push(...)` (no swap/deadly short-circuit).
3. Any remaining overlap → kill (no push).
Returns `true` = die.

## HexagonGameStatus (§5)

`double`: `total/played/pausedFrametimeAccumulator` (all init 0), `currentPause
= 0.1*60 = 6.0`, `currentIncrementTime = 0`. `float`: `customScore=0`, `pulse=75`,
`pulseDirection=1`, `pulseDelay=0`, `beatPulse=0`, `beatPulseDelay=0`,
`pulse3D=1`, `pulse3DDirection=1`, `flashEffect=0`, `radius=75`, `fastSpin=0`,
`cameraShake=0`. Flags: `hasDied=false`, `mustStateChange=None`,
`scoreInvalid=false`, `started=false`, `showPlayerTrail=true`.

```cpp
accumulateFrametime(ft /*double*/):
  total += ft;
  if (currentPause > 0.0) currentPause -= ft;      // over-run is DROPPED
  else { played += ft; currentIncrementTime += ft; }
pauseTime(s): currentPause += s*60.0;
getIncrementTimeSeconds() = currentIncrementTime/60.0;
getTotal/Played...In seconds() = .../60.0;   getTimeSeconds() = played/60.0;
resetTime(): total=played=paused=0; currentPause=0.1*60; currentIncrementTime=0;
start(): resetTime(); customScore=0; started=true;
getCurrentTP() = ms((int64)(total/60.0 * 1000.0));  getTimeTP() = ms((int64)(played/60.0*1000.0));
```
`pausedFrametimeAccumulator` is accumulated NOWHERE (always 0; getter kept).
`HRTimePoint` is ported as a `std::time::Duration` (millis-since-epoch marker)
— the game only constructs these from a ms count and compares/subtracts them.

## Ticker (§8)

`update(ft)`: `inc = ft * (float)running; current += inc; total += inc; if
(current < target) return false; ticks++; current = 0; running = loop; return
true;`. **Overflow beyond target is DROPPED** (current resets to 0, no carry).
Paused (`running=false`) → `inc=0`, never fires, never advances. `stop()` =
reset current + pause; `restart()` = reset current + resume; `setLoop(x)`.
`getTotalSecs = total/60.f`. `Ticker{80.f, false}` starts NOT running (the CPlayer
dead-effect timer; `kill()` resumes it via `restart()`).

## timeline2 / runner (§9) — wall-clock

`action` = variant of `do` / `wait_for(duration)` / `wait_until(time_point)` /
`wait_until_fn(fn->time_point)`. `append_wait_for_seconds(s)` truncates to
`ms((int)(s*1000.0))`; `append_wait_for_sixths(s) = append_wait_for_seconds(s/60.0)`.
A single runner holds ONE active `wait_for` (`_wait_start_tp`); `update(timeline,
tp)` walks actions, `waiting` halts, `proceed` advances the index; a finished
timeline is cleared + runner reset by the manager. `wait_for` compares the
elapsed wall-clock delta (>=0) against the duration.

**Every `clear()` of a timeline must be paired with a runner reset — this is NOT
optional (D-019).** `HexagonGame::setLevelData` (the `newGame` path) does
`eventTimeline.clear(); eventTimelineRunner = {};` (`HexagonGame.cpp:668-669`),
`messageTimeline... + messageTimelineRunner = {};` (`:672-673`) and
`timeline.clear(); timelineRunner = {};` (`:686-687`). A runner that survives its
timeline keeps `current_idx` / `wait_start_tp`, so the NEXT game's first
`t_wait`/`e_wait` measures against the PREVIOUS game's clock: the spawn schedule
shifts and a live run no longer replays to the same death time. Only levels that
wait early and heavily expose it (`orthoplex/bipolarity` does; `cube/apeirogon`
and `experimental/autotest0` do not) — which is exactly how it survived a 2-level
determinism harness.

## LevelStatus (§6)

`float` fields (defaults): `incTime=15.f`, `pulseMin=75.f`, `pulseMax=80.f`,
`radiusMin=72.f`, `wallSpawnDistance=<ctor>`, `speedMult/delayMult/
playerSpeedMult/swapCooldownMult/musicPitch/delayMult/beatPulseSpeedMult = 1.f`,
`incEnabled=true`, `rndSideChangesEnabled=true`, rest 0/false. `unsigned int`:
`sides/sidesMax/sidesMin = 6`. `unsigned long`: `currentIncrements = 0`.
`hasSpeedMaxLimit() = speedMax>0.f`; `hasDelayMaxLimit() = delayMax>0.f`.

## Geometry (§12) — line/circle (in oh-math)

`getLineCircleIntersection(i1,i2,p1,p2,r2)` solves `a t² + b t + c = 0` with
`a=|p2-p1|², b=2·((p2-p1)·p1), c=|p1|² - r2` (circle centered at **origin**).
`delta<0` → 0; `0<=delta<1.0e-4` → 1 (tangent, `t=-b/(2a)`); else 2 (`i1` at
`t=(-b+√δ)/2a`, `i2` at `t=(-b-√δ)/2a`). **No t∈[0,1] clamp.** `getLineCircle
ClosestIntersection` picks the nearer of i1/i2 to the given pos. `epsilon =
1.0e-4`. `pointInPolygon<N>` = even-odd ray cast; `pointInFourVertexPolygon` =
all-four cross products share a sign.

## FastVertexVectorTris (§11)

A growable ordered vertex list (`sf::Vertex = {position, color, texCoords}`,
texCoords always (0,0)). Quad = two triangles on the (nw,se) diagonal, emitted in
order **nw, sw, se, nw, se, ne**. Per-vertex-color variant (custom walls) emits
the same 6 positions with each vertex's own color (nw & se colors reused for the
two triangles). `reserve_more(n)=reserve(size*2+n)`, `reserve_more_quad(n)=
reserve_more(6n)`. The Rust port backs this with a `Vec<Vertex>` (the capacity
assert is a debug-only C++ concern; the observable contract is the ordered
sequence).

## RNG-in-update coupling (§14/§16.6 — replay determinism)

While alive, EVERY frame `HexagonGame::update` advances the game RNG by the
absolute values of five floats (truncated to u64), in this exact order:
```cpp
fixup = [](float x){ return x < 0.f ? -x : x; };   // |x|; fixup(-0.f) == 0
rng.advance(fixup(status.pulse));
rng.advance(fixup(status.pulse3D));
rng.advance(fixup(status.fastSpin));
rng.advance(fixup(status.flashEffect));
rng.advance(fixup(levelStatus.rotationSpeed));
```
Plus event-driven draws: `sideChange(rng.get_int(sidesMin, sidesMax))` (only when
`mustChangeSides && walls.empty()`) and `updateCameraShake`'s two
`rng.get_real(-i, i)` (x then y) per shaken frame. `updatePulse3D` runs even with
3D disabled (advances state → feeds the RNG fixup). These must be reproduced in
the same order or replay validation desyncs. The RNG engine + libstdc++
distribution semantics are documented in `lib-pcg.md` (crate `oh-rng`).

## Semantics that bite (pin these)

- Wall collision radius uses `radius*radius + 8.f` (add to the **square**).
- `getSign` on a float uses the `(0<x)-(x<0)` signed form (f32), NOT signum/NaN.
- The normal-wall push's `pushVel` only applies when the wall's rotation sign
  **opposes** `movementDir` (`getSign(curve._speed) != movementDir`).
- Custom-wall push loop step is `j = i++`; the escape-search step is
  `i += vxIncrement, j = i-1` (unsigned `j`). Different traversal, both must
  match exactly.
- Ticker overflow is dropped; time-pause over-run is dropped (whole frame
  counts as paused); status times are f64.
- Do NOT "fix" `updateTriangleWidthTransition` (4.5→0, not 3→-1.5).
- `setLevelData` resets **each timeline AND its runner** (§9) — a clear without
  the runner reset leaks simulation state across `newGame` and breaks replay
  parity (D-019).
- `transformHue`/`getColorFromHue`/easing live in `oh-math` (see `lib-ssvutils.md`
  for `transformHue`'s `3.14f` approximation).
