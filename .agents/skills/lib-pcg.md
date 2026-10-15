# lib-pcg — PCG random engine (`pcg32_fast`)

Source of truth: `extlibs/SSVUtils/include/SSVUtils/Internal/PCG/pcg_random.hpp`
(1792 lines, vendored PCG library, http://www.pcg-random.org/), included as a
system header via `extlibs/SSVUtils/include/SSVUtils/Internal/PCG/PCG.hpp`
(`#pragma GCC system_header`). SSVUtils submodule pin: `6200143d3f65acc90927936531ad50eff63d0d80`
("Add missing include"). Written against that commit.

Rust port: `crates/oh-rng` (`Pcg32Fast`, `RandomNumberGenerator`).
Ground-truth vectors: `.agents/plans/001-rust-port/mapping/oracle-vectors.md`
(`g++ -std=c++20 -O0` on this machine, C++ commit `1e2cba71`) and the pin tests
in `crates/oh-rng/src/lib.rs` (`#[cfg(test)] mod tests`).

## Engine identity (the one the game uses)

`pcg_random.hpp:1714`:

```cpp
typedef pcg_engines::mcg_xsh_rs_64_32 pcg32_fast;
```

`pcg_random.hpp:1579`:

```cpp
typedef mcg_base<uint64_t, uint32_t, xsh_rs_mixin> mcg_xsh_rs_64_32;
```

`pcg_random.hpp:749`:

```cpp
using mcg_base = engine<xtype, itype, output_mixin<xtype, itype>,
    output_previous, no_stream<itype>>;
```

with `output_previous = true` (the default template parameter). So:

- state type (`itype`) = `uint64_t`; result type (`xtype`) = `uint32_t`
- kind: **MCG** — `no_stream` mixin (`pcg_random.hpp:229`): `is_mcg = true`,
  `increment() == 0`, `can_specify_stream = false`
- output function: `xsh_rs_mixin::output` (`pcg_random.hpp:773`)
- output uses the **pre-advance** state (`output_previous == true`)

Sibling typedefs for reference: `pcg32` = `setseq_xsh_rr_64_32`
(`:1711`), `pcg32_oneseq`, `pcg32_unique` (`:1712-1713`); `pcg64` =
`setseq_xsl_rr_128_64` (`:1716`). Only `pcg32_fast` is used by SSVOpenHexagon
(`Core/RandomNumberGenerator.hpp` → `engine_type`).

## Exact constants

`PCG_DEFINE_CONSTANT` (`:132`) specializations:

```cpp
PCG_DEFINE_CONSTANT(uint64_t, default, multiplier, 6364136223846793005ULL)  // :151
PCG_DEFINE_CONSTANT(uint64_t, default, increment, 1442695040888963407ULL)   // :152 (unused for MCG: increment() == 0)
```

- multiplier: `6364136223846793005` (uint64) — do not change
- default ctor seed: `0xcafef00dd15ea5e5ULL` (`engine` ctor, `:477`)
- `min() == 0`, `max() == ~result_type(0)` (full u32 width, `:392-400`)
- `period_pow2() == sizeof(state_type)*8 - 2*is_mcg == 62` for MCG (`:384`)
- `wrapped()`: MCG branch is `state_ == 3` (low 2 bits stay fixed at 3, `:462`)

## Seeding (constructor, MCG branch)

`engine(itype state = 0xcafef00dd15ea5e5ULL)` (`:477-483`):

```cpp
state_(this->is_mcg ? state | state_type(3U) : bump(state + this->increment()))
```

For `pcg32_fast`: **`state = seed | 3`** (no bump on construction).

## Step + output (verbatim semantics)

```cpp
itype bump(itype state) { return state * multiplier() + increment(); } // :403
itype base_generate0()  { itype old_state = state_; state_ = bump(state_); return old_state; } // :413
result_type operator()() { if(output_previous) return this->output(base_generate0()); ... }    // :421
```

So each `next_u32()`: take the **current** state, set `state = state * 6364136223846793005 (mod 2^64)`, and return `xsh_rs_output(old_state)`.

`xsh_rs_mixin::output` (`:773-800`) with `itype = uint64_t`, `xtype = uint32_t` —
the constexpr chain (do not "simplify" the numbers):

```cpp
bits = 64; xtypebits = 32; sparebits = bits - xtypebits;            // 32
opbits = (sparebits - 5 >= 64) ? 5 : (sparebits - 4 >= 32) ? 4
       : (sparebits - 3 >= 16) ? 3 : (sparebits - 2 >= 4) ? 2
       : (sparebits - 1 >= 1) ? 1 : 0;                              // 3 (29 >= 16)
mask = (1 << opbits) - 1;             // 7
maxrandshift = mask;                  // 7
topspare = opbits;                    // 3
bottomspare = sparebits - topspare;   // 29
xshift = topspare + (xtypebits + maxrandshift) / 2;  // 3 + 19 = 22
rshift = (internal >> (bits - opbits)) & mask;       // top 3 bits, & 7
internal ^= internal >> xshift;                      // ^= internal >> 22
result = internal >> (bottomspare - maxrandshift + rshift);  // >> (22 + rshift)
```

Effective form (matches the Python re-implementation in oracle-vectors.md):

```
rshift = (state >> 61) & 7
state ^= state >> 22
out = (state >> (22 + rshift)) & 0xFFFFFFFF
```

## Jump-ahead (`advance` / `backstep` / `discard`)

`engine::advance` (`:630-658`) — Brown, "Random Number Generation with
Arbitrary Stride" (TANS Nov. 1994), fast-exponentiation style:

```cpp
acc_mult = 1; acc_plus = 0;
while (delta > 0) {
    if (delta & 1) { acc_mult *= cur_mult; acc_plus = acc_plus * cur_mult + cur_plus; }
    cur_plus = (cur_mult + 1) * cur_plus;
    cur_mult *= cur_mult;
    delta >>= 1;
}
return acc_mult * state + acc_plus;
```

All arithmetic is the full-width modular arithmetic of `itype` (u64 wrap).
For MCG, `cur_plus` stays 0, so `acc_plus` is 0 and the result is
`state * acc_mult (mod 2^64)`. `advance(delta)` sets the state;
`backstep(delta) == advance(-delta)` (unsigned wrap "goes the long way
round", per the header comment); `discard(delta) == advance(delta)`.

## Rust port API (oh-rng) — exact signatures

```rust
// crates/oh-rng/src/pcg.rs
pub const MULTIPLIER: u64 = 6364136223846793005;
pub struct Pcg32Fast { /* state: u64 */ }
impl Pcg32Fast {
    pub const fn from_seed(seed: u64) -> Self;      // state = seed | 3
    pub const fn min() -> u32;                      // 0
    pub const fn max() -> u32;                      // u32::MAX
    pub fn next_u32(&mut self) -> u32;              // output previous, then state *= M
    pub fn advance(&mut self, delta: u64);          // jump-ahead
    pub fn backstep(&mut self, delta: u64);         // advance(delta.wrapping_neg())
}

// crates/oh-rng/src/lib.rs
pub type Seed = u64;
pub type State = u64;
pub struct RandomNumberGenerator { /* seed, engine */ }
impl RandomNumberGenerator {
    pub fn new(seed: Seed) -> Self;   // C++ ctor: engine{seed} then advance(1)
    pub fn seed(&self) -> Seed;       // returns the stored seed, NOT the state
    pub fn get_int<T: IntDist>(&mut self, min: T, max: T) -> T;
    pub fn get_real<T: RealDist>(&mut self, min: T, max: T) -> T;
    pub fn advance(&mut self, delta: State);
}
```

`get_int`/`get_real` reproduce **libstdc++ 15.2** sampling (not the PCG
library's `bounded_rand`):

- `uniform_int_distribution<T>` (`bits/uniform_int_dist.h`):
  `__urange = u32(max) - u32(min)`; if `__urange == u32::MAX` the raw engine
  value is taken (full-width shortcut), otherwise Lemire's nearly-divisionless
  draw on n = `__urange + 1` with threshold `(-n) % n`:
  reject while `low < threshold`, else return `high` of `(raw * n)` u64.
  Result offset: `u32(min).wrapping_add(value) as T`.
- `uniform_real_distribution<T>` (`bits/random.tcc`): `x = generate_canonical<T, T::digits>`
  then `x * (max - min) + min` (that exact order of operations).
  `generate_canonical<f32, 24>` = `1` engine draw: `f32(x1) / 2^32`, clamped to
  `nextafter(1.0f, 0.0f)` when `>= 1.0f` (bit decrement `0x3F7FFFFF`).
  `generate_canonical<f64, 53>` = `2` engine draws:
  `(x1 + x2 * 2^32) / 2^64`, clamped to `nextafter(1.0, 0.0)` (`0x3FEFFFFFFFFFFFFF`).
  f32 and f64 consume different numbers of draws — their streams are NOT
  interleavable.

## Oracle vectors (the pin — do not regenerate "better" values)

From oracle-vectors.md (`g++ -std=c++20 -O0`, `pcg32_fast{12345u}`):

```
# engine min/max
min=0 max=4294967295

# raw engine output, pcg32_fast{12345u}()
raw=0
raw=360236480
raw=3576984158
raw=3399133164
raw=3953016716
raw=1075407150
raw=134550159
raw=1121230050

# std::uniform_int_distribution<int>{0,5}, fresh pcg32_fast{12345u}
int=0
int=4
int=4
int=5
int=1
int=0
int=1
int=0

# std::uniform_real_distribution<float>{0.f,1.f}, fresh pcg32_fast{12345u}
real=0
real=0.0838740915
real=0.832831502
real=0.791422367
real=0.920383453
real=0.250387728
real=0.0313274004
real=0.261056721

# pcg32_fast{12345u}(); advance(100); ()  (jump-ahead)
adv100=1060274396

# first output for edge seeds
seed0=0
seedmax=1023
```

Wrapper vectors (fresh `random_number_generator{12345}`, i.e. engine advanced 1
in the ctor) pinned by `crates/oh-rng/src/lib.rs` tests:

```
# get_int<int>(0,5)
w_int=0,4,4,5,1,0,1,0

# get_real<float>(0.f,1.f) — bit patterns
w_real_f32_bits=0x3DABC62E 0x3F553472 0x3F4A9AA8 0x3F6B9E40 0x3E8032D2 0x3D005129 0x3E85A93A 0x3E265F5F
# (dec: 0.0838740915, 0.832831502, 0.791422367, 0.920383453, 0.250387728, 0.0313274004, 0.261056721, 0.162473187)

# get_real<double>(0.0,1.0) — bit patterns (2 draws each; NOT interleavable with f32)
w_real_f64_bits=0x3FEAA68E4BC2AF19 0x3FED73C7F1995355 0x3FA00A251E8032D2 0x3FC4CBEBDFA16A4E 0x3FE340E28C0B8CD5
# (dec: 0.83283152386636339, 0.92038342654505334, 0.031327400181998435, 0.16247318667849914, 0.60167052605211302)

# get_int<int>(i32::MIN, i32::MAX) — full-width shortcut (raw engine value,
# offset by a via C++ unsigned add)
intfull=-2147483648,-1787247168,1429500510,1251649516

# uniform_int_distribution<int>{0,2} (non-power-of-two Lemire path)
int02=0,2,2,2,0,0,0,0,1,1,0,2
```

Also pinned: ctor behavior (first draw is the raw stream's **second** value:
`360236480`, then `3576984158`), `advance(100)` through the wrapper equals
engine `advance(101)`.

## Pitfalls

- The MCG keeps the low 2 state bits fixed at 3 (`state = seed | 3`, `bump`
  preserves them since `M` is odd and `increment == 0`).
- `output_previous` means the first output corresponds to the *seeded* state,
  not the first bumped state — this is why `random_number_generator`'s first
  draw is raw[1], not raw[0].
- `get_real<float>` / `get_real<double>` differ in draw count (1 vs 2) — mixing f32 and
  f64 calls from one engine desynchronizes the C++ stream if the C++ code did
  the same interleaving; the oracle blocks come from fresh engines per type.
- `ssvu::getRndEngine()` (Rnd.hpp) is `thread_local pcg32_fast` seeded from
  `std::random_device` — not used by the replay/RNG path, which uses
  `hg::random_number_generator` with a stored seed.
