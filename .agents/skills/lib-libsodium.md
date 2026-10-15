# lib-libsodium — libsodium as used by `Online/Sodium.*` (and `Shared.cpp`)

Source of truth: `include/SSVOpenHexagon/Online/Sodium.hpp` +
`src/SSVOpenHexagon/Online/Sodium.cpp` (the whole file, 79 lines) plus the
two call sites in `src/SSVOpenHexagon/Online/Shared.cpp`
(`makeEncryptedPacketImpl` `:690-719`, `decryptPacket` `:634-686`). The C++
build fetches libsodium via CPM `vittorioromeo/libsodium-cmake@fd76500b`
(`CMakeLists.txt:213-218`); the Rust side links **system libsodium 1.0.18**
(`libsodium-sys 0.8.7` with the `use-pkg-config` feature →
`/usr/lib/x86_64-linux-gnu/libsodium.so.23.3.0`). All seven primitives are
stable across the 1.0.x line, so the CPM version and 1.0.18 behave
identically.

Rust port: `crates/oh-net/src/sodium.rs`. Ground-truth KATs:
`mapping/oracle-vectors.md` ("Net wire vectors") pinned in
`crates/oh-net/tests/wire.rs` (cross-implementation, against the C++ probe)
+ the unit tests in `sodium.rs` itself.

## The exact C++ surface (7 primitives)

| C++ call | where | sodiumoxide 0.2.7 (libsodium 1.0.18) | Rust port (`oh_net::sodium`) |
|---|---|---|---|
| `libsodium_init()` | program start | `sodiumoxide::init()` | `ensure_initialized()` (idempotent `Once`) |
| `randombytes_buf(buf, n)` | `Sodium.cpp:16, 23, 77` | `randombytes::randombytes_into(&mut [u8])` | `generate_nonce`, `random_uint64` |
| `crypto_kx_keypair(pk, sk)` | `Sodium.cpp:23` (PS keys) | `crypto::kx::gen_keypair()` | `generate_sodium_ps_keys` |
| `crypto_kx_server_session_keys(rx, tx, s_pk, s_sk, c_pk)` | `Sodium.cpp:28-42` | `crypto::kx::server_session_keys(server_pk, server_sk, client_pk)` | `calculate_server_session_sodium_rt_keys` |
| `crypto_kx_client_session_keys(rx, tx, c_pk, c_sk, s_pk)` | `Sodium.cpp:44-58` | `crypto::kx::client_session_keys(client_pk, client_sk, server_pk)` | `calculate_client_session_sodium_rt_keys` |
| `crypto_secretbox_easy(ct, m, mlen, n, k)` | `Shared.cpp:704-706` | `crypto::secretbox::xsalsa20poly1305::seal(plain, &nonce, &key)` | `secretbox_seal` |
| `crypto_secretbox_open_easy(m, ct, ctlen, n, k)` | `Shared.cpp:676-677` | `...::open(ciphertext, &nonce, &key) -> Option<Msg>` | `secretbox_open` |
| `crypto_generichash(out, 32, in, n, key, 32)` | `Sodium.cpp:62-74` | `crypto::generichash::hash(in, Some(32), Some(&[0u8;32]))` | `sodium_hash` |

**Correction to the issue-016 contract (D-014):** the contract listed
`crypto_box_curve25519xsalsa20poly1305` and `crypto_sign_ed25519` as the
primitives — the source uses **neither**. Key pairs come from
`crypto_kx` (X25519 + BLAKE2b key *exchange*, not public-key encryption),
messages are `crypto_secretbox` (XSalsa20-Poly1305 authenticated
encryption of the session key — secret, not public), and `sodiumHash` is
`crypto_generichash` (keyed BLAKE2b). `sodiumoxide` (the D-004 choice) still
covers all of it; only the primitive list was wrong.

## Constants

From `Sodium.hpp` / the libsodium 1.0.18 headers (asserted in
`sodium.rs::tests::key_sizes_match_the_cpp_constants`):

| constant | value | meaning |
|---|---|---|
| `crypto_kx_PUBLICKEYBYTES` | 32 | PS public key (`SodiumPublicKeyArray`) |
| `crypto_kx_SECRETKEYBYTES` | 32 | PS secret key (`SodiumSecretKeyArray`) |
| `crypto_kx_SESSIONKEYBYTES` | 32 | receive *and* transmit keys (`SodiumReceiveKeyArray`/`SodiumTransmitKeyArray`) |
| `crypto_kx_SEEDBYTES` | 32 | `crypto_kx_seed_keypair` seed (test path only — the C++ layer has no seed API) |
| `crypto_secretbox_NONCEBYTES` | 24 | `SodiumNonceArray` |
| `crypto_secretbox_MACBYTES` | 16 | Poly1305 tag |

## Key exchange (the protocol's actual flow)

1. Each side generates a **PS keypair** once (`generateSodiumPSKeys`,
   `Sodium.cpp:21-26` = `crypto_kx_keypair` — random via `randombytes`).
2. The public halves travel in clear as `CTSPPublicKey` / `STCPPublicKey`
   packets (raw 32 bytes, no length prefix).
3. Each side derives its **RT session keys**:
   - client: `crypto_kx_client_session_keys(rx, tx, clientPublic, clientSecret, serverPublic)` (`Sodium.cpp:44-58`)
   - server: `crypto_kx_server_session_keys(rx, tx, serverPublic, serverSecret, clientPublic)` (`Sodium.cpp:28-42`)
   - **invariant:** `client.keyReceive == server.keyTransmit` and
     `client.keyTransmit == server.keyReceive` (pinned by the probe's
     `kx_symmetry_ok=1` and the `wire.rs` KAT).
   - Argument order trap: the *local* keys come before the *remote* public
     key in every call; the client and server calls mirror each other.
4. C++ returns `std::nullopt` on a failed session-key call; the Rust port
   returns `None` (the `from_slice`/`ok?` chain in
   `calculate_{client,server}_session_sodium_rt_keys`).

## Nonces: random per message, carried in the packet

`generateNonce()` (`Sodium.cpp:14-19`) fills **all 24 bytes** with
`randombytes_buf` for every encrypted message — there is no counter, no
nonce reuse scheme, nothing derived from the keys. The nonce travels in
clear inside `PEncryptedMsg.nonce` (24 raw bytes, first field).

Consequences:

- A byte-exact cross-implementation vector **must fix the nonce** (and the
  transmit key). The probe fakes `randombytes_buf` to emit a fixed stream
  (nonces `00..17` / `18..2F`) and pins the resulting envelopes — see
  `oracle-vectors.md` ("Net wire vectors") and `tests/wire.rs`.
- Note the C++ `makeEncryptedPacketImpl` (`Shared.cpp:690-719`) builds the
  `PEncryptedMsg` *before* encrypting: `messageLength` = inner packet size,
  `ciphertextLength = getCiphertextLength(messageLength)` = `16 +
  messageLength` (`Sodium.hpp:29-33`), then `crypto_secretbox_easy` fills a
  pre-resized ciphertext buffer.
- `libsodium`'s internal `randombytes` calls are **not interposable** from
  a probe (direct internal calls, no PLT indirection) — that is why the
  probe seeds keypairs with `crypto_kx_seed_keypair` instead of faking the
  PS-key generation.

## Secretbox (easy mode)

`crypto_secretbox_easy` (XSalsa20-Poly1305, **combined/tag-prefixed**
mode): ciphertext = 16-byte tag ‖ encrypted message, so
`ciphertextLength = messageLength + MACBYTES`. `seal` fails only for
`mlen > SIZETYPE_MAX` (4 GiB) — the C++ checks the return code
(`Shared.cpp:704-709`), the Rust port can return the ciphertext directly
(packet-sized inputs). `open` authenticates *and* decrypts; it fails on
wrong key, wrong nonce, tampered tag/message, or a ciphertext shorter than
16 bytes — the Rust `secretbox_open` maps that to `None`, which
`decryptPacket` reports as
`"Failure decrypting encrypted client message\n"`.

Decryption subtlety (`decryptPacket`, `Shared.cpp:634-686`): the C++
resizes the message buffer to the *declared* `messageLength` first, then
`append`s exactly `messageLength` bytes of it into the decrypted packet
(`:674, :683-684`). A `messageLength` larger than the true plaintext
zero-pads; smaller is an out-of-bounds read in the C++ — the Rust port
bounds-checks instead (documented deviation in `shared.rs` module docs).

## `sodiumHash` — keyed BLAKE2b, NOT keyless

`sodiumHash(in)` (`Sodium.cpp:62-74`) calls
`crypto_generichash(out, crypto_generichash_BYTES /*32*/, in, len, key,
crypto_generichash_KEYBYTES /*32*/)` with an **all-zero 32-byte key**. The
key length is part of the BLAKE2b parameter block, so keyed-with-zeros ≠
keyless. The Rust call passes the zero key explicitly
(`generichash::hash(input, Some(32), Some(&[0u8; 32]))`). Used by the login
flow for the stored password hash (`Shared.cpp` CTSPRegister/CTSPLogin
`passwordHash` fields carry the *string* form below).

KAT (cross-implementation, pinned in `tests/wire.rs`):

```
sodium_hash_oh_net=73e1c8626cbe2ed83771e3a7eaf1fc7d74c2b8fc9587c36311f453f4387de45c
sodium_hash_empty=fd7b3e075ab7e245506323f1d1b55330ccd606fb3301f4caba25022a1de50b47
```

## `randomUInt64` — host-endian reinterpretation

`Sodium.cpp:76-79`: `randombytes_buf(&result, sizeof(uint64_t))` — the 8
random bytes are reinterpreted as a native `uint64_t` (LE on the pinned
targets). KAT with the probe's fake randombytes stream
`30 31 32 33 34 35 36 37`: `random_u64=0x3736353433323130`. Rust:
`u64::from_ne_bytes`.

## `sodiumKeyToString` — decimal concatenation, no separator

`Sodium.hpp:60-71`: each byte rendered via `std::to_string(int(byte))` and
appended — `[3, 200, 1]` → `"32001"`. Not hex, not hex-with-colons, not
comma-separated. It is only a stable representation for stored keys
(DB/logs); ported verbatim as `sodium_key_to_string`.

## Deterministic KAT strategy (for any future probe)

1. PS keypairs: seed with `crypto_kx_seed_keypair(pk, sk, seed)`
   (sodiumoxide: `kx::keypair_from_seed(&Seed(seed))` — the same
   underlying call). Pinned KAT with seed patterns `0..31` / `32..63`:

   ```
   kx_server_pk=0e0216223f147143d32615a91189c288c1728cba3cc5f9f621b1026e03d83129
   kx_client_pk=99f4674ecc87c0b8e712f192b8f49e7442a9376b4875967ababa28471019a93e
   kx_client_rx=6d3e872064c89c0eac73f4713ffbbedcc286a65f7af9e993fe9dfced2404ec9c
   kx_client_tx=4fa03a98a7376a06f043151bbc4ae5c10af48b625daafccda647a83b9d27b2ea
   kx_server_rx=4fa03a98a7376a06f043151bbc4ae5c10af48b625daafccda647a83b9d27b2ea
   kx_server_tx=6d3e872064c89c0eac73f4713ffbbedcc286a65f7af9e993fe9dfced2404ec9c
   ```
2. Nonces: fake `randombytes_buf` in the C++ probe (a free function
   `randombytes_buf` in the probe TU is NOT enough for the internal calls —
   but the CTS/STC envelope nonces are the probe's own
   `generateNonce()` calls, which *do* go through the PLT, so the fake
   works there); in Rust, build the `PEncryptedMsg` variant directly with
   the fixed nonce (the `make_*_encrypted_packet` entry points draw a
   fresh random nonce — they only round-trip, never byte-pin).
3. Transmit key: any fixed 32-byte array (the probe uses `0x5A` × 32).
   `secretbox_seal`/`secretbox_open` in Rust must produce/consume the same
   ciphertexts as C++ `crypto_secretbox_{,open_}easy` — the pinned
   `cts_encrypted`/`stc_encrypted` envelopes prove it.

## sodiumoxide specifics (0.2.7, read from its vendored source)

- `crypto/kx/x25519blake2b.rs`: `gen_keypair` (random),
  `keypair_from_seed(&Seed)` → `ffi::crypto_kx_seed_keypair`
  (`x25519blake2b.rs:66`), `client_session_keys` / `server_session_keys`
  (local keys first, remote public last). Key/nonce types wrap fixed
  arrays (`PublicKey(32)`, `SecretKey(32)`, `ReceiveKey(32)`,
  `TransmitKey(32)`, `Seed(32)`); `.as_ref()` gives the `[u8]`.
- `crypto/secretbox/xsalsa20poly1305.rs`: combined mode — "the tag occupies
  the first MACBYTES bytes of the ciphertext" (`:31-32`);
  `seal`/`open` take `&Nonce(24)` / `&Key(32)`.
- `init()` must be called once (idempotent; maps to
  `sodiumoxide::init` → `sodium_init`); every `oh_net::sodium` entry point
  guards it with a `Once`.
