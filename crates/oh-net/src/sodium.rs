//! The libsodium layer: `include/SSVOpenHexagon/Online/Sodium.hpp` +
//! `src/SSVOpenHexagon/Online/Sodium.cpp`.
//!
//! The C++ code uses exactly seven libsodium primitives (all stable across
//! the 1.0.x line; the C++ build fetches libsodium through CPM —
//! `CMakeLists.txt:210-225` — and this machine's oracle probe links the
//! system libsodium 1.0.18):
//!
//! | C++ call (`Sodium.cpp` / `Shared.cpp`) | sodiumoxide 0.2.7 binding (libsodium 1.0.18, built from source by `libsodium-sys`) |
//! |---|---|
//! | `randombytes_buf(buf, n)` | `sodiumoxide::randombytes::randombytes_into` |
//! | `crypto_kx_keypair(pk, sk)` | `sodiumoxide::crypto::kx::gen_keypair` |
//! | `crypto_kx_client_session_keys(rx, tx, c_pk, c_sk, s_pk)` | `sodiumoxide::crypto::kx::client_session_keys` |
//! | `crypto_kx_server_session_keys(rx, tx, s_pk, s_sk, c_pk)` | `sodiumoxide::crypto::kx::server_session_keys` |
//! | `crypto_secretbox_easy(ct, m, mlen, n, k)` | `sodiumoxide::crypto::secretbox::xsalsa20poly1305::seal` |
//! | `crypto_secretbox_open_easy(m, ct, ctlen, n, k)` | `sodiumoxide::crypto::secretbox::xsalsa20poly1305::open` |
//! | `crypto_generichash(out, 32, in, n, key32, 32)` | `sodiumoxide::crypto::generichash::hash(in, Some(32), Some(&[0u8; 32]))` |
//!
//! Note the C++ uses `crypto_kx` (key exchange, not `crypto_box` public-key
//! encryption) for the key pair, and `crypto_secretbox` (XSalsa20-Poly1305
//! authenticated encryption of the session keys — not `crypto_box`) for the
//! messages. The issue-016 contract's "sodiumoxide" choice is correct; its
//! primitive list (`crypto_box_curve25519xsalsa20poly1305`,
//! `crypto_sign_ed25519`) is not what the source uses — see
//! `.agents/plans/001-rust-port/mapping/networking.md` (corrections).
//!
//! **Nonces are random, not counters.** `generateNonce()`
//! (`Sodium.cpp:13-17`) fills all 24 bytes with `randombytes_buf` for every
//! encrypted message, and the nonce travels in the packet itself
//! (`PEncryptedMsg.nonce`). Byte-exact cross-implementation vectors must
//! therefore fix the nonce (the pinned oracle envelope carries its nonce at
//! a fixed offset — `tests/wire.rs`).
//!
//! **`sodium_hash` is keyed-BLAKE2b with an all-zero 32-byte key.** C++
//! (`Sodium.cpp:47-56`) passes `crypto_generichash_KEYBYTES` (32) zero bytes
//! as the key — that is *not* the same as a keyless BLAKE2b (the key length
//! is part of the BLAKE2b parameter block), so the Rust call passes the
//! zero key explicitly.

use std::sync::Once;

use sodiumoxide::crypto::generichash;
use sodiumoxide::crypto::kx;
use sodiumoxide::crypto::secretbox::xsalsa20poly1305 as secretbox;

/// C++ `sodiumPublicKeyBytes` = `crypto_kx_PUBLICKEYBYTES` (32).
pub const SODIUM_PUBLIC_KEY_BYTES: usize = kx::PUBLICKEYBYTES;
/// C++ `sodiumSecretKeyBytes` = `crypto_kx_SECRETKEYBYTES` (32).
pub const SODIUM_SECRET_KEY_BYTES: usize = kx::SECRETKEYBYTES;
/// C++ `sodiumReceiveKeyBytes` = `crypto_kx_SESSIONKEYBYTES` (32).
pub const SODIUM_RECEIVE_KEY_BYTES: usize = kx::SESSIONKEYBYTES;
/// C++ `sodiumTransmitKeyBytes` = `crypto_kx_SESSIONKEYBYTES` (32).
pub const SODIUM_TRANSMIT_KEY_BYTES: usize = kx::SESSIONKEYBYTES;
/// `crypto_kx_SESSIONKEYBYTES` (32) — the session-key size shared by the
/// receive/transmit arrays (used by the secretbox helpers below).
pub const SODIUM_SESSION_KEY_BYTES: usize = kx::SESSIONKEYBYTES;
/// C++ `sodiumNonceBytes` = `crypto_secretbox_NONCEBYTES` (24).
pub const SODIUM_NONCE_BYTES: usize = secretbox::NONCEBYTES;
/// `crypto_secretbox_MACBYTES` (16) — the tag length.
pub const SODIUM_MAC_BYTES: usize = secretbox::MACBYTES;

/// C++ `SodiumPublicKeyArray` (`std::array<unsigned char, 32>`).
pub type SodiumPublicKeyArray = [u8; SODIUM_PUBLIC_KEY_BYTES];
/// C++ `SodiumSecretKeyArray` (`std::array<unsigned char, 32>`).
pub type SodiumSecretKeyArray = [u8; SODIUM_SECRET_KEY_BYTES];
/// C++ `SodiumReceiveKeyArray` (the session key this side receives with).
pub type SodiumReceiveKeyArray = [u8; SODIUM_RECEIVE_KEY_BYTES];
/// C++ `SodiumTransmitKeyArray` (the session key this side encrypts with).
pub type SodiumTransmitKeyArray = [u8; SODIUM_TRANSMIT_KEY_BYTES];
/// C++ `SodiumNonceArray` (`std::array<unsigned char, 24>`).
pub type SodiumNonceArray = [u8; SODIUM_NONCE_BYTES];

static ENSURE_INITIALIZED: Once = Once::new();

/// C++ `libsodium_init()` (called once at program start): make the
/// sodiumoxide thread-safety precondition hold. Idempotent.
fn ensure_initialized() {
    ENSURE_INITIALIZED.call_once(|| {
        sodiumoxide::init()
            .expect("libsodium initialization failed (sodiumoxide::init / sodium_init)");
    });
}

/// C++ `getCiphertextLength(messageLength)`
/// (`Sodium.hpp:30-33`): `crypto_secretbox_MACBYTES + messageLength` (the
/// easy-mode ciphertext is tag-prefixed).
#[must_use]
pub const fn get_ciphertext_length(message_length: u64) -> u64 {
    SODIUM_MAC_BYTES as u64 + message_length
}

/// C++ `generateNonce()` (`Sodium.cpp:13-17`) — 24 fresh random bytes
/// (`randombytes_buf`); **not** derived from a counter.
#[must_use]
pub fn generate_nonce() -> SodiumNonceArray {
    ensure_initialized();
    let mut nonce = [0u8; SODIUM_NONCE_BYTES];
    sodiumoxide::randombytes::randombytes_into(&mut nonce);
    nonce
}

/// C++ `SodiumPSKeys` (`Sodium.hpp:44-47`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SodiumPsKeys {
    /// C++ `keyPublic`.
    pub key_public: SodiumPublicKeyArray,
    /// C++ `keySecret`.
    pub key_secret: SodiumSecretKeyArray,
}

/// C++ `SodiumRTKeys` (`Sodium.hpp:49-52`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SodiumRtKeys {
    /// C++ `keyReceive` — the session key this side *decrypts* with.
    pub key_receive: SodiumReceiveKeyArray,
    /// C++ `keyTransmit` — the session key this side *encrypts* with.
    pub key_transmit: SodiumTransmitKeyArray,
}

/// C++ `generateSodiumPSKeys()` (`Sodium.cpp:19-23`) — `crypto_kx_keypair`.
#[must_use]
pub fn generate_sodium_ps_keys() -> SodiumPsKeys {
    ensure_initialized();
    let (public_key, secret_key) = kx::gen_keypair();
    SodiumPsKeys {
        key_public: public_key
            .as_ref()
            .try_into()
            .expect("kx public key is 32 bytes"),
        key_secret: secret_key
            .as_ref()
            .try_into()
            .expect("kx secret key is 32 bytes"),
    }
}

/// C++ `calculateServerSessionSodiumRTKeys(serverPSKeys, clientPublicKey)`
/// (`Sodium.cpp:25-40`) — `crypto_kx_server_session_keys(keyReceive,
/// keyTransmit, serverPublic, serverSecret, clientPublic)`.
///
/// Returns `None` exactly where C++ returns `std::nullopt` (the session-key
/// call fails, or the raw key arrays cannot be wrapped — the latter is
/// unreachable for fixed-size arrays).
#[must_use]
pub fn calculate_server_session_sodium_rt_keys(
    server: &SodiumPsKeys,
    client_public: &SodiumPublicKeyArray,
) -> Option<SodiumRtKeys> {
    ensure_initialized();
    let server_pk = kx::PublicKey::from_slice(&server.key_public)?;
    let server_sk = kx::SecretKey::from_slice(&server.key_secret)?;
    let client_pk = kx::PublicKey::from_slice(client_public)?;
    let (rx, tx) = kx::server_session_keys(&server_pk, &server_sk, &client_pk).ok()?;
    Some(SodiumRtKeys {
        key_receive: rx.as_ref().try_into().expect("session key is 32 bytes"),
        key_transmit: tx.as_ref().try_into().expect("session key is 32 bytes"),
    })
}

/// C++ `calculateClientSessionSodiumRTKeys(clientPSKeys, serverPublicKey)`
/// (`Sodium.cpp:42-57`) — `crypto_kx_client_session_keys(keyReceive,
/// keyTransmit, clientPublic, clientSecret, serverPublic)`.
#[must_use]
pub fn calculate_client_session_sodium_rt_keys(
    client: &SodiumPsKeys,
    server_public: &SodiumPublicKeyArray,
) -> Option<SodiumRtKeys> {
    ensure_initialized();
    let client_pk = kx::PublicKey::from_slice(&client.key_public)?;
    let client_sk = kx::SecretKey::from_slice(&client.key_secret)?;
    let server_pk = kx::PublicKey::from_slice(server_public)?;
    let (rx, tx) = kx::client_session_keys(&client_pk, &client_sk, &server_pk).ok()?;
    Some(SodiumRtKeys {
        key_receive: rx.as_ref().try_into().expect("session key is 32 bytes"),
        key_transmit: tx.as_ref().try_into().expect("session key is 32 bytes"),
    })
}

/// C++ `sodiumKeyToString` (`Sodium.hpp:59-70`) — each byte rendered as its
/// **decimal** string, concatenated with no separator
/// (e.g. `[3, 200, 1]` → `"32001"`). The format is unambiguous nowhere; it
/// is only a stable DB/log representation (the client and server both use
/// it for their stored keys), so it is ported verbatim.
#[must_use]
pub fn sodium_key_to_string(key: &[u8]) -> String {
    key.iter().map(|b| b.to_string()).collect()
}

/// C++ `sodiumHash(s)` (`Sodium.cpp:63-73`) — `crypto_generichash`
/// (BLAKE2b) with the default 32-byte output and the all-zero 32-byte key
/// (see the module docs for why the zero key is not a no-op).
///
/// C++ returns a `std::string` of the raw digest bytes; Rust returns the
/// fixed 32-byte array (the digest length is the constant 32).
#[must_use]
pub fn sodium_hash(input: &[u8]) -> [u8; 32] {
    ensure_initialized();
    let digest = generichash::hash(input, Some(32), Some(&[0u8; 32]))
        .expect("32-byte generichash output is always valid");
    digest
        .as_ref()
        .try_into()
        .expect("digest is 32 bytes (out_len = Some(32))")
}

/// C++ `randomUInt64()` (`Sodium.cpp:75-79`) — 8 random bytes reinterpreted
/// as a host-endian `uint64_t` (`randombytes_buf(&result, 8)`).
#[must_use]
pub fn random_uint64() -> u64 {
    ensure_initialized();
    let mut bytes = [0u8; 8];
    sodiumoxide::randombytes::randombytes_into(&mut bytes);
    u64::from_ne_bytes(bytes)
}

/// C++ `crypto_secretbox_easy` (the call in `Shared.cpp:
/// makeEncryptedPacketImpl`) — XSalsa20-Poly1305, tag-prefixed ciphertext
/// (16-byte tag then the encrypted message).
///
/// C++ reports failure via the return code; `crypto_secretbox_easy` fails
/// only for `mlen > SIZETYPE_MAX` (4 GiB), so the Rust call returns the
/// ciphertext directly (the caller sizes are packet-sized).
#[must_use]
pub fn secretbox_seal(
    plain: &[u8],
    nonce: &SodiumNonceArray,
    key: &[u8; SODIUM_SESSION_KEY_BYTES],
) -> Vec<u8> {
    ensure_initialized();
    let nonce = secretbox::Nonce::from_slice(nonce).expect("nonce is 24 bytes");
    let key = secretbox::Key::from_slice(key).expect("key is 32 bytes");
    secretbox::seal(plain, &nonce, &key)
}

/// C++ `crypto_secretbox_open_easy` (the call in `Shared.cpp:
/// decryptPacket`) — verifies the tag and decrypts; `None` on any
/// authentication failure (wrong key, tampered ciphertext, or a ciphertext
/// shorter than the tag).
#[must_use]
pub fn secretbox_open(
    ciphertext: &[u8],
    nonce: &SodiumNonceArray,
    key: &[u8; SODIUM_SESSION_KEY_BYTES],
) -> Option<Vec<u8>> {
    ensure_initialized();
    let nonce = secretbox::Nonce::from_slice(nonce).expect("nonce is 24 bytes");
    let key = secretbox::Key::from_slice(key).expect("key is 32 bytes");
    secretbox::open(ciphertext, &nonce, &key).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fixed 32-byte seed for the `crypto_kx_seed_keypair` test path
    /// (sodiumoxide exposes it; the C++ layer has no seed path, so this is
    /// a test-only deterministic stand-in for `generate_sodium_ps_keys`).
    fn seeded_ps_keys(seed: u8) -> SodiumPsKeys {
        let (public_key, secret_key) = kx::keypair_from_seed(&kx::Seed([seed; kx::SEEDBYTES]));
        SodiumPsKeys {
            key_public: public_key.as_ref().try_into().unwrap(),
            key_secret: secret_key.as_ref().try_into().unwrap(),
        }
    }

    #[test]
    fn key_sizes_match_the_cpp_constants() {
        // C++ Sodium.hpp: crypto_kx_PUBLICKEYBYTES/SECRETKEYBYTES/
        // SESSIONKEYBYTES (32/32/32), crypto_secretbox_NONCEBYTES (24),
        // crypto_secretbox_MACBYTES (16).
        assert_eq!(SODIUM_PUBLIC_KEY_BYTES, 32);
        assert_eq!(SODIUM_SECRET_KEY_BYTES, 32);
        assert_eq!(SODIUM_RECEIVE_KEY_BYTES, 32);
        assert_eq!(SODIUM_TRANSMIT_KEY_BYTES, 32);
        assert_eq!(SODIUM_NONCE_BYTES, 24);
        assert_eq!(SODIUM_MAC_BYTES, 16);
        assert_eq!(get_ciphertext_length(0), 16);
        assert_eq!(get_ciphertext_length(5), 21);
    }

    #[test]
    fn key_exchange_is_symmetric() {
        // The invariant sodiumoxide documents (and the C++ protocol
        // relies on): client rx == server tx and client tx == server rx.
        let client = seeded_ps_keys(1);
        let server = seeded_ps_keys(2);

        let client_rt = calculate_client_session_sodium_rt_keys(&client, &server.key_public)
            .expect("valid key material");
        let server_rt = calculate_server_session_sodium_rt_keys(&server, &client.key_public)
            .expect("valid key material");

        assert_eq!(client_rt.key_receive, server_rt.key_transmit);
        assert_eq!(client_rt.key_transmit, server_rt.key_receive);
    }

    #[test]
    fn random_keypairs_exchange_too() {
        let client = generate_sodium_ps_keys();
        let server = generate_sodium_ps_keys();
        let client_rt =
            calculate_client_session_sodium_rt_keys(&client, &server.key_public).unwrap();
        let server_rt =
            calculate_server_session_sodium_rt_keys(&server, &client.key_public).unwrap();
        assert_eq!(client_rt.key_receive, server_rt.key_transmit);
        assert_eq!(client_rt.key_transmit, server_rt.key_receive);
    }

    fn fixed_nonce() -> SodiumNonceArray {
        [7u8; 24]
    }

    fn fixed_key() -> [u8; SODIUM_SESSION_KEY_BYTES] {
        [0x5Au8; SODIUM_SESSION_KEY_BYTES]
    }

    #[test]
    fn secretbox_round_trip_with_fixed_key_and_nonce() {
        let message = b"oh-net fixed-nonce vector";
        let ciphertext = secretbox_seal(message, &fixed_nonce(), &fixed_key());
        assert_eq!(ciphertext.len(), message.len() + SODIUM_MAC_BYTES);
        assert_eq!(
            secretbox_open(&ciphertext, &fixed_nonce(), &fixed_key()),
            Some(message.to_vec())
        );
    }

    #[test]
    fn secretbox_open_rejects_wrong_key_and_tampering() {
        let message = b"authenticated";
        let mut ciphertext = secretbox_seal(message, &fixed_nonce(), &fixed_key());

        let mut wrong_key = fixed_key();
        wrong_key[0] ^= 1;
        assert_eq!(
            secretbox_open(&ciphertext, &fixed_nonce(), &wrong_key),
            None
        );

        let last = ciphertext.len() - 1;
        ciphertext[last] ^= 0x80;
        assert_eq!(
            secretbox_open(&ciphertext, &fixed_nonce(), &fixed_key()),
            None
        );

        // A nonce change also fails authentication.
        let mut wrong_nonce = fixed_nonce();
        wrong_nonce[3] ^= 1;
        assert_eq!(
            secretbox_open(&ciphertext, &wrong_nonce, &fixed_key()),
            None
        );

        // Shorter than the 16-byte tag.
        assert_eq!(
            secretbox_open(&ciphertext[..15], &fixed_nonce(), &fixed_key()),
            None
        );
    }

    #[test]
    fn key_to_string_is_decimal_concatenation_without_separators() {
        // C++: std::to_string(int(byte)) per byte, appended.
        assert_eq!(sodium_key_to_string(&[3, 200, 1]), "32001");
        assert_eq!(sodium_key_to_string(&[0, 0]), "00");
        assert_eq!(sodium_key_to_string(&[]), "");
        assert_eq!(sodium_key_to_string(&[255]), "255");
    }

    #[test]
    fn sodium_hash_is_deterministic_and_32_bytes() {
        let a = sodium_hash(b"salt+test-password");
        let b = sodium_hash(b"salt+test-password");
        assert_eq!(a, b);
        assert_eq!(a.len(), 32);
        // Different input → different digest.
        assert_ne!(a, sodium_hash(b"salt+other-password"));
        // (The byte-exact KAT is pinned by the C++ oracle — tests/wire.rs.)
    }

    #[test]
    fn generate_nonce_is_24_bytes_and_fresh() {
        let a = generate_nonce();
        let b = generate_nonce();
        assert_eq!(a.len(), 24);
        // 24 random bytes colliding has probability ~2^-192.
        assert_ne!(a, b);
    }

    #[test]
    fn random_uint64_runs() {
        // Non-deterministic; just exercise the path.
        let _ = random_uint64();
    }
}
