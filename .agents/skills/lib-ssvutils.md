# lib-ssvutils — SSVUtils core subset used by the port

Source of truth: `extlibs/SSVUtils` (vendored, C++14, STL-only; AFL-3.0).
Submodule pin: `6200143d3f65acc90927936531ad50eff63d0d80` ("Add missing
include"). Written against that commit. The game pulls it in as
`ssvoh_find_extlib_for_target(SSVOpenHexagonLib SSVUtils)` (CMakeLists.txt:474).

Rust port: `crates/oh-math` (`ssvu::*`), one module per C++ header
(`crates/oh-math/src/ssvu/{math,containers,frametime}.rs`).
The RNG engine itself is documented in `lib-pcg.md`.

Fidelity rule (DECISIONS.md D-000): every C++ `float` → `f32`, `double` →
`f64`; constants copied verbatim; **no floating-point expression is
reordered**. Oracle-pinned values: `.agents/plans/001-rust-port/mapping/
oracle-vectors.md` + `crates/oh-math/tests/oracle_utils.rs` (g++ -std=c++20 -O0
probe of the real C++ sources).

## Frametime (`include/SSVUtils/Core/Common/Frametime.hpp`)

```cpp
using FT = float;                                                    // :19
using FTDuration = std::chrono::duration<FT, std::milli>;            // :24
constexpr float secondsFTRatio{60.f};                                // :27
template <typename T> constexpr T getFTToSeconds(T mFT) noexcept;    // :31 — mFT / secondsFTRatio
template <typename T> constexpr T getSecondsToFT(T mSeconds) noexcept; // :38 — mSeconds * secondsFTRatio
template <typename T> constexpr T getFTToFPS(T mFT) noexcept;        // :45 — secondsFTRatio / mFT
```

Rust (`ssvu::frametime`): `type FT = f32`, `SECONDS_FT_RATIO: f32 = 60.0`,
`get_ft_to_seconds` / `get_seconds_to_ft` / `get_ft_to_fps` (f32 forms; the C++
templates are float-only in game usage). The C++ game treats frametime as
frames at 60 fps; `FT` is plain `float`.

## Math (`include/SSVUtils/Core/Utils/Math.hpp`) — the ported subset

Constants (exact literals — clippy allows are pinned on the Rust side for
these; do NOT substitute `std::f32::consts::*`, which are different values):

```cpp
constexpr float pi{3.14159265359f};            // :18
constexpr float piHalf{1.57079632679f};        // :21  (NOT ported — unused)
constexpr float tau{6.28318530718f};           // :24
constexpr float radDegRatio{0.01745329251f};   // :27
```

```cpp
// :34 — unsigned overload: return 0 < mX;                     (bool as int)
// :45 — signed overload:   return (0 < mX) - (mX < 0);       1 / -1 / 0
template <typename T> inline constexpr int getSign(const T& mX, ...) noexcept;
// NaN input: both comparisons false → returns 0 (Rust port keeps the two-
// comparison form; f32::signum would return NaN instead — do not swap).

// :55/:64/:74 — in-place; C++ clamp has assert(mMin <= mMax)
if (mX < mMin) mX = mMin;              // clampMin
if (mX > mMax) mX = mMax;              // clampMax
if (mX < mMin) mX = mMin; else if (mX > mMax) mX = mMax;   // clamp

// :89/:101/:115 — value-returning; getClamped ternary (exact order):
//   return CT(mX) < CT(mMin) ? CT(mMin) : (CT(mX) > CT(mMax) ? CT(mMax) : CT(mX));
// NaN input: both comparisons false → returns mX (NaN). On the pinned Rust
// toolchain f32::clamp matches this exactly (NaN -> NaN, min-then-max order).

// :292 — mathematically correct mod (assumes mUB > 0):
//   auto result(((mVal % mUB) + mUB) % mUB);   // getMod(-2, 12) == 10
template <typename T1, typename T2> inline std::common_type_t<T1, T2> getMod(
    const T1& mVal, const T2& mUB);
// :310 — wrap into [mLB, mUB) (assumes mLB < mUB); C++ verbatim:
//   const auto& rangeSize(mUB - mLB);
//   if (mVal < mLB) mVal += rangeSize * ((mLB - mVal) / rangeSize + 1);
//   return mLB + ((mVal - mLB) % rangeSize);
template <typename T1, typename T2, typename T3>
inline std::common_type_t<T1, T2, T3> getMod(T1 mVal, const T2& mLB, const T3& mUB) noexcept;
```

```cpp
// :329 — (mX1 - mX2)^2 + (mY1 - mY2)^2
template <...> inline constexpr ... getDistSquaredEuclidean(
    const T1& mX1, const T2& mY1, const T3& mX2, const T4& mY2) noexcept;
// :342 — std::sqrt(getDistSquaredEuclidean(...))
template <...> inline constexpr ... getDistEuclidean(
    const T1& mX1, const T2& mY1, const T3& mX2, const T4& mY2) noexcept;
// :146/:156 — toRad: mX * radDegRatio;  toDeg: mX / radDegRatio
```

Rust mapping (`ssvu::math`): `PI`, `TAU`, `RAD_DEG_RATIO`, `get_sign`
(generic `PartialOrd + Copy + From<u8>`; the signed expression degenerates to
the unsigned overload for unsigned types), `clamp` (in place, `debug_assert!`
min ≤ max), `get_clamped`, `get_mod` (2-arg), `get_mod_range` (3-arg),
`get_dist_squared_euclidean`, `get_dist_euclidean` (f32), `to_rad`.

**NOT ported** (unused by the game — see issue 001 report "known gaps"):
`getCycledValue` (`:132`), `getWrapRad`/`getWrapDeg`, `getRotatedDeg`/
`getRotatedRad`, the 1D/2D/3D index helpers (`get1DIdxFrom2D`, …),
`getDistManhattan`/`getDistChebyshev`, `getRadTowards`/`getDegTowards`,
`getDistRad`/`getDistDeg`, `getMap`, `getLerp`. If a later issue needs one,
port it verbatim from this file.

## Containers (`include/SSVUtils/Core/Utils/Containers.hpp`)

Ported subset (game uses only these two):

```cpp
// :204 — find-based linear search
template <typename TC, typename TV>
inline bool contains(const TC& mContainer, const TV& mValue) noexcept;
//   return find(mContainer, mValue) != std::end(mContainer);

// :263/:274 — non-const uses operator[], const uses .at() (same index)
template <typename TC, typename TIdx>
inline auto& getByModIdx(TC& mContainer, const TIdx& mIdx) noexcept;
//   return mContainer[getMod(mIdx, mContainer.size())];
```

Rust (`ssvu::containers`): `contains<T: PartialEq>(&[T], &T) -> bool`,
`get_by_mod_idx<T, I: AsContainerIdx>(&[T], I) -> &T`. `AsContainerIdx`
implements `static_cast<std::size_t>(mIdx)` semantics for i8..i64/u8..u64/
usize (negative values wrap two's-complement, as in C++).

**NOT ported**: `eraseRemoveIf` (`:252`), `iota`/`fill`/`anyOf`/`find`/`rotate`/`findIf`/`remove`/`removeIf`/`lowerBound`/`containsAnyIf`/`containsAnyOf`/`containsAllOf`/`trimL`/`trimR`/`trimLR` (+`getTrim*`) and `shuffle` (`:417`) — all unused by the game.

## Rnd (`include/SSVUtils/Core/Utils/Rnd.hpp`) — context only

```cpp
// :17-24 — thread_local engine, seeded once from std::random_device
namespace Impl { struct PCGInit {
    pcg_extras::seed_seq_from<std::random_device> seedSrc;
    pcg32_fast rng{seedSrc};
}; }
inline auto& getRndEngine() noexcept { thread_local Impl::PCGInit i; return i.rng; }  // :26

// :44 — assert(min < max); uniform_int_distribution<CT>{min, max - 1}  (NOTE: max-1!)
template <typename T1, typename T2> inline auto getRndI(const T1& mMin, const T2& mMax) noexcept;
// :58 — assert(min <= max); uniform_real_distribution<CT>(min, max)
template <typename T1, typename T2> inline auto getRndR(const T1& mMin, const T2& mMax) noexcept;
// :71 — normal_distribution<CT>(mean, deviation)
template <typename T1, typename T2> inline auto getRndRNormal(...) noexcept;
// :83 — RndDistUniformI<T>{0, 1}(engine) > 0 ? -1 : 1
template <typename T = int> inline T getRndSign() noexcept;
```

The game's *deterministic* RNG path does NOT use `ssvu::Rnd`: it uses
`hg::random_number_generator` (`include/SSVOpenHexagon/Core/
RandomNumberGenerator.{hpp,cpp}`) — a plain `pcg32_fast` wrapped with
`get_int`/`get_real` (libstdc++ distributions) — ported in `crates/oh-rng`
(see `lib-pcg.md` for engine + distribution semantics and vectors).

## Semantics that bite (pin these in any port)

- `getSign` NaN → `0` (comparison arithmetic), not NaN.
- `getClamped`/`getSaturated`-style clamps propagate NaN to the result
  (both comparisons false → input returned). Rust `f32::clamp` on the pinned
  toolchain (1.97.1) does the same; comparison order is min-first, then max.
- `getMod` (2-arg) assumes positive `mUB`; (3-arg) assumes `mLB < mUB`.
  Negative `mIdx` in `getByModIdx` wraps via `size_t` conversion (e.g.
  `-1 % 3 == 0` on 3 items).
- `getRndI` is `[min, max)` (it subtracts 1), `getRndR` is `[min, max]`
  (both-inclusive) — the C++ distributions' own bounds.
- `toRad` multiplies by the *constant* `0.01745329251f` (not
  `pi/180`); `toDeg` divides by it. `to_rad(180.0f32)` must equal the exact
  f32 `3.14159274` (pinned by `oracle_utils.rs`).
- `transformHue` (game Utils, `include/SSVOpenHexagon/Utils/Color.hpp:33`)
  uses the deliberate approximation `3.14f`, not pi — kept verbatim in the
  Rust port.
