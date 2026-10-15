# lib-sfml-packet — SFML `sf::Packet` wire codec (custom fork)

Source of truth: the pinned fork `vittorioromeo/SFML@dacceddbd1adee49c7bfd120266e200800550394`
(`CMakeLists.txt:108-112`), file `src/SFML/Network/Packet.cpp` (620 lines) +
`include/SFML/Network/Packet.hpp`. Written against that commit. This is the
**full** `sf::Packet` surface — `lib-sfml.md` covers the fork's drawing layer
only and cross-references this file for the codec.

Rust port: `crates/oh-replay/src/packet.rs` (`Packet`), re-exported as
`oh_net::packet::Packet` (`crates/oh-net/src/packet.rs`). Ground truth:
`mapping/oracle-vectors.md` ("SFML `Packet` encoding rules" + "Replay
serialization" + "Net wire vectors"); pinned in
`crates/oh-replay/tests/` and `crates/oh-net/tests/wire.rs`.

**Reconciliation (D-012):** the `oracle-vectors.md` pin "BE ints + u32-BE
string lengths, raw host-endian floats, bool = 1 byte" is **confirmed
correct** against the fork source below — no correction needed.

## Encoding table (the compatibility surface)

| type | wire bytes | write (`Packet.cpp`) | read (`Packet.cpp`) |
|---|---|---|---|
| `bool` | 1 byte (0/1) | `:393` | `:119` — any nonzero → `true` |
| `int8_t`/`uint8_t` | 1 raw byte | `:401` / `:409` | `:130` / `:143` |
| `int16_t` | 2 **big-endian** (bit pattern) | `:417` (`htons`) | `:156` (`htons`) |
| `uint16_t` | 2 **big-endian** | `:426` (`htons`) | `:170` (`htons`) |
| `int32_t` | 4 **big-endian** (bit pattern) | `:435` (`htonl`) | `:184` (`htonl`) |
| `uint32_t` | 4 **big-endian** | `:444` (`htonl`) | `:198` (`htonl`) |
| `int64_t` | 8 **big-endian** (bit pattern) | `:453` (manual shifts) | `:212` (manual shifts) |
| `uint64_t` | 8 **big-endian** | `:473` (manual shifts) | `:234` (manual shifts) |
| `float` | 4 raw — **host-endian** (LE on x86), NOT swapped | `:493` (`memcpy`) | `:256` (`memcpy`) |
| `double` | 8 raw — **host-endian** (LE on x86), NOT swapped | `:501` (`memcpy`) | `:269` (`memcpy`) |
| `const char*` | u32-BE length (`strlen`) + raw bytes | `:509` | — (`:282` reads into `char*` with a caller-supplied length) |
| `std::string` | u32-BE length + raw bytes | `:521` | `:303` |
| `const wchar_t*` | u32-BE length (`wcslen`) + **4 bytes big-endian per `char32`** | `:537` | `:324` |
| `std::wstring` | u32-BE length + 4-byte-BE per char | `:551` | `:347` |
| `sf::String` | u32-BE length + 4-byte-BE per codepoint | `:569` | `:370` |
| `append(data, n)` | raw bytes at the write position | `:63` | `readData`-style raw take |

String-length truncation: the length is
`static_cast<std::uint32_t>(data.size())` (`:525`) — a >4 GiB string wraps;
the bytes are only appended when `length > 0` (`:530`), so a wrapped length 0
still appends nothing. The Rust `write_str` mirrors the cast
(`crates/oh-replay/src/packet.rs`).

Floats are the trap: everything integral goes through `htons`/`htonl`/manual
big-endian, but `float`/`double` are plain `memcpy` (`:493-509`, `:256-281`) —
host-endian, so **little-endian on the pinned x86_64 targets**. The Rust port
uses explicit `to_le_bytes`/`from_le_bytes` so the bytes stay deterministic
regardless of build host. (Same reason the replay *file* hex starts
`05000000` (LE u32, raw `memcpy` in `Replay.cpp`) while the *packet* form
starts `0000000000000054` (BE u64) — see `oracle-vectors.md`.)

## Failure ("poisoning") semantics

`checkSize` (`:589-594`):

```cpp
bool Packet::checkSize(std::size_t size)
{
    m_isValid = m_isValid && (m_readPos + size <= m_data.size());
    return m_isValid;
}
```

Every `operator>>` guards with `if(checkSize(sizeof(T))) { ... } return *this;`
— so on an out-of-range read the value is **left unchanged**, the read
position does **not** advance, and `m_isValid` is **sticky** (once false,
every later read fails too). Consequences used by the port:

- `operator bool()` (`:112-117`) returns `m_isValid` — SFML idiom
  `if(!(p >> x))` == "packet is poisoned".
- `endOfPacket()` (`:105-110`) is `m_readPos >= m_data.size()` (independent
  of validity).
- `clear()` (`:82-87`) empties the data, resets both positions, and sets
  `m_isValid = true` — **clearing un-poisons** the packet.
- Because a failed read neither advances nor recovers, C++ code that keeps
  extracting after a failure appends one error line per remaining field
  (this is the `Online/Shared.cpp` decode model — see
  `crates/oh-net/src/shared.rs` module docs and the pinned negative vectors
  in `oracle-vectors.md`, "Net wire vectors").

`getData()`/`getDataSize()` (`:91-103`) return the whole buffer / its size,
independent of the read position. `append(data, n)` (`:63-80`) copies at the
write position, resizing the buffer if the write position is past the end.

## C++ → Rust API map

| C++ (`sf::Packet& p`) | Rust (`oh_replay::packet::Packet`, `&mut p`) |
|---|---|
| `p << v` (any typed op) | `p.write_{bool,i8,u8,i16,u16,i32,u32,i64,u64,f32,f64,str}(...)` |
| `p >> v` (any typed op) | `p.read_...(...) -> Option<T>` — `None` iff the C++ would set the poison flag; position unchanged, value unchanged (no out-param in Rust) |
| `p.append(data, n)` | `p.append(&[u8])` |
| `p.clear()` | `p.reset()` (data + both positions; the Rust `Packet` has no separate validity flag — `read_*`/`take` model it via `None`) |
| `p.getData()` / `getDataSize()` | `p.data()` / `p.size()` |
| `p >> end` / `operator bool` | `p.available()` / the `Option` results |

Note the Rust type has **no sticky flag**: a failed `read_*`/`take` simply
returns `None` and leaves `read_pos` put. That is observationally identical
for every C++ call sequence the port reproduces (a poisoned C++ packet fails
all later reads; the Rust reader is only ever continued with by the port's
own decode loop, which records the errors the same way). If a future port
needs to observe "is this packet still valid?" mid-stream, use
`p.available()` / the `Option` chain rather than adding a flag.

## Verification anchors

- Fork pin: `CMakeLists.txt:108-112` (`vittorioromeo/SFML` at
  `dacceddbd1adee49c7bfd120266e200800550394`); the repo's C++ oracle shim
  (`tools/oracle/shim/SFML/Network/Packet.hpp`) encodes exactly the table
  above and the poison semantics, and was cross-checked against the fork
  headers (see `oracle-vectors.md`, "Gameplay geometry" note).
- Byte-exact pins: `crates/oh-replay/tests/` (replay packet forms `PHEX`/
  `CZHEX`/`STREAMHEX`) and `crates/oh-net/tests/wire.rs` (every net packet
  type + the encrypted envelopes, from `tools/oracle/net_probe.cpp`).
