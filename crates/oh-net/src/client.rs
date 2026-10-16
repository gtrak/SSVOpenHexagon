//! The `Core/HexagonClient` port (992 lines + the header) — the online
//! client: connection, the PS/RT-key handshake, register/login/logout,
//! delete account, score requests, started-game, compressed-replay upload,
//! heartbeat keep-alive, and the event queue the UI drains.
//!
//! ## Contract correction (D-014)
//!
//! The issue-017 contract said the C++ client is "a background thread +
//! queues" and to "reproduce the same model". It is not: `HexagonClient`
//! owns a non-blocking `sf::TcpSocket` (fork `TcpSocket.cpp`) and does all
//! I/O from the game loop — `update()` (called once per frame,
//! `HexagonClient.cpp:777-794`) drives `sendHeartbeatIfNecessary()` (the
//! 45-second window, `:508-530`) and one `receiveDataFromServer()`
//! (`:532-775`); the only "queue" is the `std::deque<Event>` of
//! *application* events the caller drains with `pollEvent()`
//! (`:975-984`). There is no thread anywhere in the class (grep-verified:
//! no `std::thread` use in `HexagonClient.{hpp,cpp}`; the `<thread>`
//! include at `:19` serves only the 50 ms ticket-poll sleep). The port
//! reproduces that pull model exactly: `update()` + `poll_event()`.
//!
//! ## Socket layer
//!
//! SFML's `TcpSocket` is split here into the two layers the C++ actually
//! uses, so the client logic and the transport are each testable:
//!
//! - [`NetSocket`] — the raw byte surface (the C++
//!   `TcpSocket::send/receive(void*, size, count&)` + `connect` +
//!   `disconnect`, fork `TcpSocket.cpp:234-305, 343-365, 213-219`);
//!   [`StdTcpSocket`] is the default implementation (`std::net::TcpStream`,
//!   non-blocking after connect, mirroring the C++ blocking-connect with a
//!   0.5 s timeout then `setBlocking(false)` — `HexagonClient.cpp:91-116`).
//! - [`FramedSocket`] — the *packet* surface (the C++
//!   `TcpSocket::send/receive(Packet&)`, fork `TcpSocket.cpp:309-424`):
//!   u32-`htonl` length-prefixed frames, the `m_blockToSendBuffer` +
//!   `Packet::m_sendPos` partial-send resume (`:320-362`), and the
//!   `m_pendingPacket` size-then-payload accumulation that survives
//!   across `receive` calls (`:376-422`) and is reset by
//!   `disconnect()` (`:213-219`).
//!
//! ## Documented deviations
//!
//! - **Logging**: the C++ `clog`/`SSVOH_CLOG*` calls (the `ssvu::lo` app
//!   logger) and the `_verbose` member are dropped — they are diagnostic
//!   only, no protocol behaviour depends on them, and `oh-net` has no log
//!   dependency.
//! - **Time**: every `HRClock::now()` (monotonic, `Utils/Clock.hpp:14`) and
//!   the 50 ms `std::this_thread::sleep_for` go through [`ClientClock`]
//!   (default `Instant::now` + `thread::sleep`) so the 45 s heartbeat
//!   window and the 16-try ticket poll are testable without real waiting.
//! - **Steam**: the C++ `Steam::steam_manager&` is the [`SteamManager`]
//!   trait (the four methods the constructor polls, `Steam.hpp:41-66`).
//! - **`sodiumHash` digest → `passwordHash` string**: the C++ stores the
//!   32 raw digest bytes in a `std::string` (raw bytes; not UTF-8 in
//!   general). The wire codec (`shared.rs`, issue 16) models the field as a
//!   `String` and encodes `len()` + `as_bytes()` — so the port wraps the
//!   digest with `from_utf8_unchecked` (byte-exact: `len()` is the 32-byte
//!   length and `as_bytes()` is the raw digest, exactly the C++
//!   `p << std::string` of the raw bytes). The string is never re-interpreted
//!   as text.
//! - **Password salt**: the C++ uses the secret
//!   `Online/SecretPasswordSalt.hpp` literal when that header exists in the
//!   build, else the fallback `"salt"` (`HexagonClient.cpp:799-811`). The
//!   header is absent from the source tree (grep-verified), so the current
//!   C++ build hashes `salt + password`; the port pins that fallback as
//!   [`PASSWORD_SALT`].
//! - **`getLoginName`** returns `Option<&String>` (the C++ returns
//!   `const std::optional<std::string>&`).
//!
//! ## Session state
//!
//! The "client session state" the plan assigns to this issue is this
//! class's own state — the pieces `issue 16` (the wire codec) did not own:
//! `_state`, `_loginToken`/`_loginName`, `_serverPublicKey`,
//! `_clientRTKeys`, `_levelValidatorsSupportedByServer`, `_ticketSteamID`,
//! and the event deque. Every transition is below, each with its C++
//! citation; none of the `Shared.cpp` encoding is duplicated (all packets
//! go through `crate::shared` + `crate::sodium`).

use std::collections::HashSet;
use std::io::{Read as _, Write as _};
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

use crate::packet::Packet;
use crate::protocol::{GAME_VERSION, PROTOCOL_VERSION};
use crate::shared::{
    decode_server_to_client_packet, make_client_to_server_encrypted_packet,
    make_client_to_server_packet, ClientToServer, ServerToClient,
};
use crate::sodium::{
    calculate_client_session_sodium_rt_keys, generate_sodium_ps_keys, sodium_hash, SodiumPsKeys,
    SodiumPublicKeyArray, SodiumRtKeys,
};

// ---------------------------------------------------------------------------
// Socket layer
// ---------------------------------------------------------------------------

/// C++ `sf::Socket::Status` for a raw *send* (fork `Socket.hpp:63-69`).
/// `Partial` carries the bytes actually put on the wire (the C++ reports
/// them through the `size_t& sent` out-parameter, `TcpSocket.cpp:234-268`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendResult {
    /// C++ `Status::Done` — the whole range was sent.
    Done,
    /// C++ `Status::Partial` — some, not all, bytes were sent.
    Partial(usize),
    /// C++ `Status::NotReady` — would block, nothing sent.
    NotReady,
    /// C++ `Status::Error`.
    Error,
    /// C++ `Status::Disconnected`.
    Disconnected,
}

/// C++ `sf::Socket::Status` for a raw *receive* — `Done` carries the bytes
/// read (the C++ `size_t& received`, `TcpSocket.cpp:272-305`; a `recv()` of
/// 0 maps to `Disconnected`, `:297-300`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecvResult {
    /// C++ `Status::Done` — `n` bytes read (`n > 0`).
    Done(usize),
    /// C++ `Status::NotReady` — would block, nothing read.
    NotReady,
    /// C++ `Status::Error`.
    Error,
    /// C++ `Status::Disconnected` — peer closed.
    Disconnected,
}

/// The raw non-blocking socket surface — the C++ `sf::TcpSocket` byte-level
/// API (fork `TcpSocket.cpp`). Implementations are non-blocking after a
/// successful [`Self::connect`].
pub trait NetSocket {
    /// C++ `TcpSocket::connect(ip, port, timeout)` called with the socket in
    /// *blocking* mode (the C++ sets it at `HexagonClient.cpp:104`), with a
    /// 0.5 s timeout (the C++ `sf::seconds(0.5)`, `:107`). The implementation
    /// must leave the socket non-blocking on success (the C++ flips to
    /// non-blocking right after, `:114`).
    fn connect(&mut self, addr: SocketAddr, timeout: Duration) -> RecvResult;

    /// C++ `TcpSocket::send(void*, size, sent&)` — non-blocking. Loops until
    /// every byte is sent or the socket would block (fork
    /// `TcpSocket.cpp:234-268`: `Partial` only when some bytes went out).
    fn send(&mut self, buf: &[u8]) -> SendResult;

    /// C++ `TcpSocket::receive(void*, size, received&)` — a single non-blocking
    /// `recv` of up to `buf.len()` bytes (fork `TcpSocket.cpp:272-305`).
    fn recv(&mut self, buf: &mut [u8]) -> RecvResult;

    /// C++ `TcpSocket::disconnect()` — closes the socket AND resets the
    /// pending-receive state (fork `TcpSocket.cpp:213-219`); a raw socket has
    /// no pending state, so implementations just close.
    fn disconnect(&mut self);
}

/// The default [`NetSocket`] — `std::net::TcpStream` in non-blocking mode.
#[derive(Debug, Default)]
pub struct StdTcpSocket {
    stream: Option<TcpStream>,
}

impl StdTcpSocket {
    /// C++ `sf::TcpSocket` default-constructed (unconnected).
    #[must_use]
    pub fn new() -> Self {
        Self { stream: None }
    }
}

impl NetSocket for StdTcpSocket {
    fn connect(&mut self, addr: SocketAddr, timeout: Duration) -> RecvResult {
        match TcpStream::connect_timeout(&addr, timeout) {
            Ok(stream) => {
                // C++ `setBlocking(false)` (`HexagonClient.cpp:114`).
                if stream.set_nonblocking(true).is_ok() {
                    self.stream = Some(stream);
                    RecvResult::Done(0)
                } else {
                    RecvResult::Error
                }
            }
            // C++: NotReady on timeout / Error otherwise — the C++ consumer
            // only distinguishes `Done` from "not Done"
            // (`HexagonClient.cpp:107-113`), so both map to Error.
            Err(_) => RecvResult::Error,
        }
    }

    fn send(&mut self, buf: &[u8]) -> SendResult {
        let Some(stream) = self.stream.as_mut() else {
            return SendResult::Error;
        };
        if buf.is_empty() {
            // C++ `send(void*)` with `size == 0` → Error (TcpSocket.cpp:239-243).
            return SendResult::Error;
        }
        let mut sent = 0;
        loop {
            // C++ `::send()` — this Rust's `TcpStream` has no inherent
            // `send`; `Write::write` has the same non-blocking contract
            // (bytes written, or `WouldBlock`).
            match stream.write(&buf[sent..]) {
                Ok(n) => {
                    if n == 0 {
                        // C++ cannot observe a 0-byte partial send
                        // (`::send` < 0 → status, > 0 → bytes); treat it like
                        // a would-block at whatever offset we reached.
                        return if sent > 0 {
                            SendResult::Partial(sent)
                        } else {
                            SendResult::NotReady
                        };
                    }
                    sent += n;
                    if sent == buf.len() {
                        return SendResult::Done;
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    // C++: `(status == NotReady) && sent > 0 → Partial`,
                    // `sent == 0 → NotReady` (TcpSocket.cpp:259-263).
                    return if sent > 0 {
                        SendResult::Partial(sent)
                    } else {
                        SendResult::NotReady
                    };
                }
                Err(_) => return SendResult::Error,
            }
        }
    }

    fn recv(&mut self, buf: &mut [u8]) -> RecvResult {
        let Some(stream) = self.stream.as_mut() else {
            return RecvResult::Error;
        };
        match stream.read(buf) {
            // C++ `recv() == 0` → Disconnected (TcpSocket.cpp:297-300).
            Ok(0) => RecvResult::Disconnected,
            Ok(n) => RecvResult::Done(n),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => RecvResult::NotReady,
            Err(_) => RecvResult::Error,
        }
    }

    fn disconnect(&mut self) {
        self.stream = None;
    }
}

/// The C++ `sf::TcpSocket` *packet* layer (fork `TcpSocket.cpp:309-424`):
/// u32-`htonl` length-prefixed frames over a raw [`NetSocket`], with the
/// C++'s two piece of persistent state:
///
/// - the send side: `m_blockToSendBuffer` (the size-prefixed frame) +
///   `Packet::m_sendPos` (the resume offset after a `Partial`,
///   `TcpSocket.cpp:320-362`);
/// - the receive side: `m_pendingPacket` (the 4-byte size, then the
///   payload, accumulated across `receive_packet` calls, `:376-422`);
///   `disconnect` resets the receive side (`:213-219`).
pub struct FramedSocket {
    inner: Box<dyn NetSocket>,
    /// C++ `m_blockToSendBuffer` (size-prefixed frame being sent).
    send_buf: Vec<u8>,
    /// C++ `Packet::m_sendPos` (resume offset into `send_buf`).
    send_pos: usize,
    /// C++ `m_pendingPacket.Size` (the 4 size bytes, `ntohl`-ed once all 4
    /// arrive).
    recv_size: [u8; 4],
    /// C++ `m_pendingPacket.SizeReceived`.
    recv_size_pos: usize,
    /// C++ `m_pendingPacket.Data` (payload accumulated so far).
    recv_data: Vec<u8>,
}

impl FramedSocket {
    /// Wrap a raw socket (the C++ default-constructs a `sf::TcpSocket`
    /// member, `HexagonClient.hpp:88`).
    #[must_use]
    pub fn new(inner: Box<dyn NetSocket>) -> Self {
        Self {
            inner,
            send_buf: Vec::new(),
            send_pos: 0,
            recv_size: [0; 4],
            recv_size_pos: 0,
            recv_data: Vec::new(),
        }
    }

    /// Forward to the raw socket (the C++ `connect` is done on the
    /// `sf::TcpSocket` before any packet traffic, `HexagonClient.cpp:104-114`).
    pub fn connect(&mut self, addr: SocketAddr, timeout: Duration) -> RecvResult {
        self.inner.connect(addr, timeout)
    }

    /// C++ `sf::TcpSocket::disconnect()` — close AND reset the pending
    /// receive state (fork `TcpSocket.cpp:213-219`). The C++ does NOT reset
    /// the send side on disconnect; the port mirrors that (it is
    /// unreachable in practice: the C++ never reconnects — `connect()` on an
    /// initialized socket fails, `HexagonClient.cpp:398-400`).
    pub fn disconnect(&mut self) {
        self.inner.disconnect();
        self.recv_size_pos = 0;
        self.recv_data.clear();
    }

    /// C++ `sf::TcpSocket::send(Packet& p)` (fork `TcpSocket.cpp:309-365`).
    ///
    /// The C++ rebuilds `m_blockToSendBuffer` from the CURRENT packet
    /// contents on every call (`packet.onSend(size)`, `:320-336`) and
    /// resumes from `Packet::m_sendPos` (`:356-357`) — the offset survives
    /// even if the contents changed since the last call. (`HexagonClient`
    /// never changes the packet between a failed send and its retry —
    /// `makeClientToServerPacket` runs once, then `sendPacketRecursive`
    /// re-sends the same bytes, `HexagonClient.cpp:118-149` — so in
    /// practice the rebuilt buffer is identical.)
    fn send_packet(&mut self, p: &Packet) -> SendResult {
        let payload = p.data();
        let mut buf = Vec::with_capacity(4 + payload.len());
        buf.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        buf.extend_from_slice(payload);
        self.send_buf = buf;
        if self.send_pos > self.send_buf.len() {
            // The C++ `size - m_sendPos` unsigned underflow makes the send
            // fail; `Error` stands in for the unrepresentable slice.
            return SendResult::Error;
        }
        self.flush_send_buf()
    }

    /// The C++ send loop's tail: one `send(void*)` of the pending range
    /// (fork `TcpSocket.cpp:234-268`), with the `m_sendPos` bookkeeping
    /// (`:359-363`).
    fn flush_send_buf(&mut self) -> SendResult {
        // C++: `send(m_blockToSendBuffer.data() + m_sendPos, size - m_sendPos, sent)`.
        match self.inner.send(&self.send_buf[self.send_pos..]) {
            SendResult::Done => {
                self.send_buf.clear();
                self.send_pos = 0;
                SendResult::Done
            }
            SendResult::Partial(n) => {
                self.send_pos += n;
                if self.send_pos == self.send_buf.len() {
                    self.send_buf.clear();
                    self.send_pos = 0;
                    SendResult::Done
                } else {
                    SendResult::Partial(n)
                }
            }
            // C++: NotReady/Error/Disconnected are returned; `m_sendPos` is
            // preserved for the next `send_packet` call with the same data.
            other => other,
        }
    }

    /// C++ `sf::TcpSocket::receive(Packet& p)` (fork `TcpSocket.cpp:368-424`):
    /// `p` is **cleared first** (the C++ `packet.clear()`, `:371`); on
    /// success the fully-received payload lands in `p` (the C++
    /// `packet.onReceive`, `:415-417`) and `Done` is returned; a partial
    /// frame stays in `self` (the C++ `m_pendingPacket`) for the next call.
    fn receive_packet(&mut self, p: &mut Packet) -> RecvResult {
        // C++ `packet.clear()` — the user packet never holds partial data.
        p.reset();

        // Size phase (C++ `:376-387`): loop until the 4 size bytes arrived.
        if self.recv_size_pos < 4 {
            loop {
                let chunk = &mut self.recv_size[self.recv_size_pos..];
                match self.inner.recv(chunk) {
                    RecvResult::Done(n) => {
                        self.recv_size_pos += n;
                        if self.recv_size_pos == 4 {
                            break;
                        }
                    }
                    // C++: `if (status != Done) return status;`
                    other => return other,
                }
            }
        }
        let packet_size = u32::from_be_bytes(self.recv_size) as usize;

        // Payload phase (C++ `:392-410`): 1024-byte chunks until complete.
        while self.recv_data.len() < packet_size {
            let size_to_get = (packet_size - self.recv_data.len()).min(1024);
            let mut buf = vec![0u8; size_to_get];
            match self.inner.recv(&mut buf) {
                RecvResult::Done(n) => self.recv_data.extend_from_slice(&buf[..n]),
                // C++: `if (status != Done) return status;`
                other => return other,
            }
        }

        // Complete (C++ `:413-422`).
        if !self.recv_data.is_empty() {
            p.append(&self.recv_data);
        }
        self.recv_data.clear();
        self.recv_size_pos = 0;
        RecvResult::Done(0)
    }
}

// ---------------------------------------------------------------------------
// Time + Steam surfaces
// ---------------------------------------------------------------------------

/// The C++ time surface `HexagonClient` touches: `HRClock::now()`
/// (`Utils/Clock.hpp:14` — the monotonic high-resolution clock; the
/// 45-second heartbeat window, `HexagonClient.cpp:511`) and
/// `std::this_thread::sleep_for` (the 50 ms ticket-poll pause, `:64`).
/// Default: real time; tests inject a controlled pair.
pub struct ClientClock {
    now: Box<dyn Fn() -> Instant>,
    sleep: Box<dyn Fn(Duration)>,
}

impl Default for ClientClock {
    fn default() -> Self {
        Self::new()
    }
}

impl ClientClock {
    /// Real time (`Instant::now` + `thread::sleep`).
    #[must_use]
    pub fn new() -> Self {
        Self {
            now: Box::new(Instant::now),
            sleep: Box::new(std::thread::sleep),
        }
    }

    /// A controlled clock (tests).
    #[must_use]
    pub fn custom(now: impl Fn() -> Instant + 'static, sleep: impl Fn(Duration) + 'static) -> Self {
        Self {
            now: Box::new(now),
            sleep: Box::new(sleep),
        }
    }

    /// C++ `HRClock::now()`.
    fn now(&self) -> Instant {
        (self.now)()
    }

    /// C++ `std::this_thread::sleep_for`.
    fn sleep(&self, d: Duration) {
        (self.sleep)(d);
    }
}

/// C++ `hg::Steam::steam_manager` (`Core/Steam.hpp:41-66`) — the four
/// methods the client constructor polls for the validated Steam ID
/// (`HexagonClient.cpp:55-89`).
pub trait SteamManager {
    /// C++ `got_encrypted_app_ticket_response() const noexcept`.
    fn got_encrypted_app_ticket_response(&self) -> bool;
    /// C++ `run_callbacks()`.
    fn run_callbacks(&mut self);
    /// C++ `got_encrypted_app_ticket() const noexcept`.
    fn got_encrypted_app_ticket(&self) -> bool;
    /// C++ `get_ticket_steam_id() const noexcept` → `std::optional<uint64_t>`.
    fn get_ticket_steam_id(&self) -> Option<u64>;
}

// ---------------------------------------------------------------------------
// State + events
// ---------------------------------------------------------------------------

/// C++ `HexagonClient::State` (`HexagonClient.hpp:41-49`) — the numeric
/// values are the C++ enumerators (they are a stable identity in logs and
/// the ported tests).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum State {
    Disconnected = 0,
    InitError = 1,
    Connecting = 2,
    ConnectionError = 3,
    Connected = 4,
    LoggedIn = 5,
    /// C++ `LoggedIn_Ready`.
    LoggedInReady = 6,
}

/// C++ `HexagonClient::Event` — the `std::variant` of 15 event structs
/// (`HexagonClient.hpp:52-77`). The C++ `E`-prefix is dropped (the enum
/// name carries it); the variant order matches the C++ variant order.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// C++ `EConnectionSuccess`.
    ConnectionSuccess,
    /// C++ `EConnectionFailure`.
    ConnectionFailure {
        /// C++ `error`.
        error: String,
    },
    /// C++ `EKicked`.
    Kicked,
    /// C++ `ERegistrationSuccess`.
    RegistrationSuccess,
    /// C++ `ERegistrationFailure`.
    RegistrationFailure {
        /// C++ `error`.
        error: String,
    },
    /// C++ `ELoginSuccess`.
    LoginSuccess,
    /// C++ `ELoginFailure`.
    LoginFailure {
        /// C++ `error`.
        error: String,
    },
    /// C++ `ELogoutSuccess`.
    LogoutSuccess,
    /// C++ `ELogoutFailure`.
    LogoutFailure,
    /// C++ `EDeleteAccountSuccess`.
    DeleteAccountSuccess,
    /// C++ `EDeleteAccountFailure`.
    DeleteAccountFailure {
        /// C++ `error`.
        error: String,
    },
    /// C++ `EReceivedTopScores`.
    ReceivedTopScores {
        /// C++ `levelValidator`.
        level_validator: String,
        /// C++ `scores`.
        scores: Vec<crate::db_records::ProcessedScore>,
    },
    /// C++ `EReceivedOwnScore`.
    ReceivedOwnScore {
        /// C++ `levelValidator`.
        level_validator: String,
        /// C++ `score`.
        score: crate::db_records::ProcessedScore,
    },
    /// C++ `EGameVersionMismatch`.
    GameVersionMismatch,
    /// C++ `EProtocolVersionMismatch`.
    ProtocolVersionMismatch,
}

// ---------------------------------------------------------------------------
// The client
// ---------------------------------------------------------------------------

/// C++ `hg::HexagonClient` (`Core/HexagonClient.hpp`). See the module docs
/// for the contract correction (pull model, not a background thread) and
/// the documented deviations.
pub struct HexagonClient {
    // C++ `_steamManager` (the C++ holds a reference; the port owns the
    // trait object for its lifetime).
    steam_manager: Box<dyn SteamManager>,
    // C++ `_ticketSteamID`.
    ticket_steam_id: Option<u64>,
    // C++ `_serverIp` + `_serverPort`.
    server_addr: SocketAddr,
    // C++ `_socket` (the `sf::TcpSocket` — packet framing included).
    socket: FramedSocket,
    // C++ `_socketConnected`.
    socket_connected: bool,
    // C++ `_packetBuffer`.
    packet_buffer: Packet,
    // C++ `_errorOss` (always `str("")`-ed before its one use, so a field
    // mirrors the C++ member-for-member).
    error_oss: String,
    // C++ `_lastHeartbeatTime` (`HRTimePoint`). The C++ default is the epoch
    // (→ "a heartbeat is due"); the port uses an `Instant` captured at
    // construction — unobservable: the first heartbeat is sent in the
    // constructor's `connect()`, and `update()` early-returns without a
    // socket.
    last_heartbeat_time: Instant,
    // C++ `_clientPSKeys`.
    client_ps_keys: SodiumPsKeys,
    // C++ `_serverPublicKey`.
    server_public_key: Option<SodiumPublicKeyArray>,
    // C++ `_clientRTKeys`.
    client_rt_keys: Option<SodiumRtKeys>,
    // C++ `_state`.
    state: State,
    // C++ `_loginToken`.
    login_token: Option<u64>,
    // C++ `_loginName`.
    login_name: Option<String>,
    // C++ `_events`.
    events: std::collections::VecDeque<Event>,
    // C++ `_levelValidatorsSupportedByServer`.
    level_validators_supported_by_server: HashSet<String>,
    // The C++ `HRClock::now()` / `sleep_for` surface.
    clock: ClientClock,
}

/// C++ `saltAndHashPwd` (`HexagonClient.cpp:799-811`): `sodiumHash(salt +
/// password)` — 32 raw digest bytes, wrapped as the wire `String` (see the
/// module docs for the `from_utf8_unchecked` note).
fn salt_and_hash_pwd(password: &str) -> String {
    let salted = format!("{PASSWORD_SALT}{password}");
    let digest = sodium_hash(salted.as_bytes());
    // The C++ `std::string` holds the raw digest bytes (not UTF-8 in
    // general); the wire codec encodes `len()` + `as_bytes()`, so an
    // unchecked wrap is byte-exact. Soundness: the string is only ever
    // handed to the wire codec (which treats it as bytes) or compared —
    // never re-interpreted as UTF-8 text.
    unsafe { String::from_utf8_unchecked(digest.to_vec()) }
}

/// The C++ fallback password salt (`HexagonClient.cpp:805-807` — the
/// `#else` branch; the `SecretPasswordSalt.hpp` header is absent from the
/// tree, so this is what the C++ build hashes with).
const PASSWORD_SALT: &str = "salt";

impl HexagonClient {
    /// C++ `HexagonClient(steamManager, serverIp, serverPort)`
    /// (`HexagonClient.cpp:437-470`): generate the PS keys, poll the Steam
    /// ticket (→ `InitError` on failure), then `connect()`.
    #[must_use]
    pub fn new(steam_manager: Box<dyn SteamManager>, server_ip: SocketAddr) -> Self {
        Self::with_socket(
            steam_manager,
            server_ip,
            Box::new(StdTcpSocket::new()),
            ClientClock::new(),
        )
    }

    /// Test/alternate-transport constructor: the same C++ constructor body
    /// over an injected raw socket and clock.
    #[must_use]
    pub fn with_socket(
        steam_manager: Box<dyn SteamManager>,
        server_ip: SocketAddr,
        socket: Box<dyn NetSocket>,
        clock: ClientClock,
    ) -> Self {
        let client_ps_keys = generate_sodium_ps_keys();
        let mut client = Self {
            steam_manager,
            ticket_steam_id: None,
            server_addr: server_ip,
            socket: FramedSocket::new(socket),
            socket_connected: false,
            packet_buffer: Packet::new(),
            error_oss: String::new(),
            last_heartbeat_time: Instant::now(),
            client_ps_keys,
            server_public_key: None,
            client_rt_keys: None,
            state: State::Disconnected,
            login_token: None,
            login_name: None,
            events: std::collections::VecDeque::new(),
            level_validators_supported_by_server: HashSet::new(),
            clock,
        };

        if !client.initialize_ticket_steam_id() {
            // C++ `:462-466`: log + `State::InitError`, no `connect()`.
            client.state = State::InitError;
            return client;
        }

        client.connect();
        client
    }

    // -- C++ private helpers -------------------------------------------------

    /// C++ `initializeTicketSteamID` (`HexagonClient.cpp:55-89`).
    fn initialize_ticket_steam_id(&mut self) -> bool {
        // C++ `:58-69`: poll `run_callbacks` + 50 ms sleep; `tries > 15`
        // (i.e. 16 failed polls) → "Never got Steam ID validation response".
        let mut tries = 0;
        while !self.steam_manager.got_encrypted_app_ticket_response() {
            self.steam_manager.run_callbacks();
            self.clock.sleep(Duration::from_millis(50));
            tries += 1;
            if tries > 15 {
                return false;
            }
        }
        // C++ `:71-75`: a response without a valid ticket → fail.
        if !self.steam_manager.got_encrypted_app_ticket() {
            return false;
        }
        match self.steam_manager.get_ticket_steam_id() {
            Some(id) => {
                self.ticket_steam_id = Some(id);
                true
            }
            // C++ `:77-81`: "No Steam ID received from encrypted app ticket".
            None => false,
        }
    }

    /// C++ `initializeTcpSocket` (`HexagonClient.cpp:91-116`).
    fn initialize_tcp_socket(&mut self) -> bool {
        if self.socket_connected {
            return false;
        }
        // C++ `setBlocking(true)` + `connect(ip, port, sf::seconds(0.5))`
        // (`:104-107`) — the [`NetSocket::connect`] contract does the
        // blocking-timeout connect and leaves the socket non-blocking
        // (the C++ `setBlocking(false)`, `:114`).
        if self
            .socket
            .connect(self.server_addr, Duration::from_millis(500))
            != RecvResult::Done(0)
        {
            self.socket_connected = false;
            return false;
        }
        self.socket_connected = true;
        true
    }

    /// C++ `sendPacketRecursive(tries, p)` (`HexagonClient.cpp:118-149`).
    fn send_packet_recursive(&mut self, tries: i32) -> bool {
        if tries > 5 {
            return false;
        }
        match self.socket.send_packet(&self.packet_buffer) {
            SendResult::Done => true,
            // C++ `SSVOH_ASSERT(status == Partial)` then recurse.
            SendResult::Partial(_) => self.send_packet_recursive(tries + 1),
            SendResult::NotReady => false,
            SendResult::Error => false,
            SendResult::Disconnected => false,
        }
    }

    /// C++ `recvPacketRecursive(tries, p)` (`HexagonClient.cpp:152-188`).
    fn recv_packet_recursive(&mut self, tries: i32, p: &mut Packet) -> bool {
        if tries > 5 {
            return false;
        }
        match self.socket.receive_packet(p) {
            RecvResult::Done(_) => true,
            RecvResult::NotReady => false,
            // C++: Error/Disconnected → log, `disconnect()`, fail.
            RecvResult::Error | RecvResult::Disconnected => {
                self.disconnect();
                false
            }
        }
    }

    /// C++ `sendPacket(p)` (`HexagonClient.cpp:192-194`).
    fn send_packet(&mut self) -> bool {
        self.send_packet_recursive(0)
    }

    /// C++ `recvPacket(p)` (`HexagonClient.cpp:197-199`).
    fn recv_packet(&mut self, p: &mut Packet) -> bool {
        self.recv_packet_recursive(0, p)
    }

    /// C++ `sendUnencrypted<T>(data)` (`HexagonClient.cpp:202-212`).
    fn send_unencrypted(&mut self, data: &ClientToServer) -> bool {
        if !self.socket_connected {
            return false;
        }
        make_client_to_server_packet(&mut self.packet_buffer, data);
        self.send_packet()
    }

    /// C++ `sendEncrypted<T>(data)` (`HexagonClient.cpp:214-234`).
    fn send_encrypted(&mut self, data: &ClientToServer) -> bool {
        if !self.socket_connected {
            return false;
        }
        let Some(rt_keys) = self.client_rt_keys.as_ref() else {
            // C++ "Tried to send encrypted message without RT keys".
            return false;
        };
        // C++ checks `makeClientToServerEncryptedPacket`'s bool (false only
        // for > 4 GiB messages, `Shared.cpp:741-748`) — the port is
        // infallible (documented in `shared.rs`).
        make_client_to_server_encrypted_packet(
            &rt_keys.key_transmit,
            &mut self.packet_buffer,
            data,
        );
        self.send_packet()
    }

    /// C++ `sendHeartbeat` (`HexagonClient.cpp:236-244`).
    fn send_heartbeat(&mut self) -> bool {
        if !self.send_unencrypted(&ClientToServer::CTSPHeartbeat) {
            return false;
        }
        self.last_heartbeat_time = self.clock.now();
        true
    }

    /// C++ `sendDisconnect` (`HexagonClient.cpp:247-249`).
    fn send_disconnect(&mut self) -> bool {
        self.send_unencrypted(&ClientToServer::CTSPDisconnect)
    }

    /// C++ `sendPublicKey` (`HexagonClient.cpp:252-254`).
    fn send_public_key(&mut self) -> bool {
        self.send_unencrypted(&ClientToServer::CTSPPublicKey {
            key: self.client_ps_keys.key_public,
        })
    }

    /// C++ `sendRegister` (`HexagonClient.cpp:257-268`).
    fn send_register(&mut self, steam_id: u64, name: &str, password_hash: &str) -> bool {
        self.send_encrypted(&ClientToServer::CTSPRegister {
            steam_id,
            name: name.to_string(),
            password_hash: password_hash.to_string(),
        })
    }

    /// C++ `sendLogin` (`HexagonClient.cpp:271-282`).
    fn send_login(&mut self, steam_id: u64, name: &str, password_hash: &str) -> bool {
        self.send_encrypted(&ClientToServer::CTSPLogin {
            steam_id,
            name: name.to_string(),
            password_hash: password_hash.to_string(),
        })
    }

    /// C++ `sendLogout` (`HexagonClient.cpp:285-288`).
    fn send_logout(&mut self, steam_id: u64) -> bool {
        self.send_encrypted(&ClientToServer::CTSPLogout { steam_id })
    }

    /// C++ `sendDeleteAccount` (`HexagonClient.cpp:291-302`).
    fn send_delete_account(&mut self, steam_id: u64, password_hash: &str) -> bool {
        self.send_encrypted(&ClientToServer::CTSPDeleteAccount {
            steam_id,
            password_hash: password_hash.to_string(),
        })
    }

    /// C++ `sendRequestTopScores` (`HexagonClient.cpp:304-315`).
    fn send_request_top_scores(&mut self, login_token: u64, level_validator: &str) -> bool {
        self.send_encrypted(&ClientToServer::CTSPRequestTopScores {
            login_token,
            level_validator: level_validator.to_string(),
        })
    }

    /// C++ `sendRequestOwnScore` (`HexagonClient.cpp:317-328`).
    fn send_request_own_score(&mut self, login_token: u64, level_validator: &str) -> bool {
        self.send_encrypted(&ClientToServer::CTSPRequestOwnScore {
            login_token,
            level_validator: level_validator.to_string(),
        })
    }

    /// C++ `sendRequestTopScoresAndOwnScore`
    /// (`HexagonClient.cpp:330-341`).
    fn send_request_top_scores_and_own_score(
        &mut self,
        login_token: u64,
        level_validator: &str,
    ) -> bool {
        self.send_encrypted(&ClientToServer::CTSPRequestTopScoresAndOwnScore {
            login_token,
            level_validator: level_validator.to_string(),
        })
    }

    /// C++ `sendStartedGame` (`HexagonClient.cpp:344-355`).
    fn send_started_game(&mut self, login_token: u64, level_validator: &str) -> bool {
        self.send_encrypted(&ClientToServer::CTSPStartedGame {
            login_token,
            level_validator: level_validator.to_string(),
        })
    }

    /// C++ `sendCompressedReplay` (`HexagonClient.cpp:357-369`). The C++
    /// uses `levelValidator` only for the verbose log (dropped — see the
    /// module docs); the wire packet carries it as a field regardless.
    fn send_compressed_replay(
        &mut self,
        login_token: u64,
        _level_validator: &str,
        compressed_replay_file: &oh_replay::CompressedReplayFile,
    ) -> bool {
        self.send_encrypted(&ClientToServer::CTSPCompressedReplay {
            login_token,
            compressed_replay_file: compressed_replay_file.clone(),
        })
    }

    /// C++ `sendRequestServerStatus` (`HexagonClient.cpp:372-381`).
    fn send_request_server_status(&mut self, login_token: u64) -> bool {
        self.send_encrypted(&ClientToServer::CTSPRequestServerStatus { login_token })
    }

    /// C++ `sendReady` (`HexagonClient.cpp:384-392`).
    fn send_ready(&mut self, login_token: u64) -> bool {
        self.send_encrypted(&ClientToServer::CTSPReady { login_token })
    }

    fn add_event(&mut self, e: Event) {
        self.events.push_back(e);
    }

    /// C++ `connectedAndInState(s)` (`HexagonClient.cpp:963-966`).
    fn connected_and_in_state(&self, s: State) -> bool {
        self.socket_connected && self.state == s
    }

    /// C++ `connectedAndInAnyState(s0, s1)` (`HexagonClient.cpp:969-972`).
    fn connected_and_in_any_state(&self, s0: State, s1: State) -> bool {
        self.socket_connected && (self.state == s0 || self.state == s1)
    }

    // -- C++ public API ------------------------------------------------------

    /// C++ `connect()` (`HexagonClient.cpp:395-434`).
    pub fn connect(&mut self) -> bool {
        self.state = State::Connecting;

        if self.socket_connected {
            // C++ `:398-400` — no event, just `fail()`.
            return false;
        }

        if !self.initialize_tcp_socket() {
            // C++ `failEvent("initializing TCP socket")` (`:403-412`).
            self.add_event(Event::ConnectionFailure {
                error: "Failure connecting, error initializing TCP socket".to_string(),
            });
            self.state = State::ConnectionError;
            return false;
        }

        if !self.send_heartbeat() {
            // C++ `failEvent("sending first heartbeat")`.
            self.add_event(Event::ConnectionFailure {
                error: "Failure connecting, error sending first heartbeat".to_string(),
            });
            self.state = State::ConnectionError;
            return false;
        }

        if !self.send_public_key() {
            // C++ `failEvent("sending public key")`.
            self.add_event(Event::ConnectionFailure {
                error: "Failure connecting, error sending public key".to_string(),
            });
            self.state = State::ConnectionError;
            return false;
        }

        self.add_event(Event::ConnectionSuccess);
        self.state = State::Connected;
        true
    }

    /// C++ `disconnect()` (`HexagonClient.cpp:482-505`).
    pub fn disconnect(&mut self) {
        // C++ `setBlocking(true)` (`:485`) — a no-op on the port's
        // non-blocking stream (the send/recv paths do not depend on it).
        if (self.state == State::LoggedIn || self.state == State::LoggedInReady)
            && self.ticket_steam_id.is_some()
        {
            // C++ `(void)sendLogout(*_ticketSteamID)` (`:488-491`).
            let _ = self.send_logout(self.ticket_steam_id.expect("checked above"));
        }

        if self.state == State::Connected
            || self.state == State::LoggedIn
            || self.state == State::LoggedInReady
        {
            // C++ `(void)sendDisconnect()` (`:493-496`).
            let _ = self.send_disconnect();
        }

        self.socket.disconnect();
        self.socket_connected = false;

        self.state = State::Disconnected;
    }

    /// C++ `sendHeartbeatIfNecessary()` (`HexagonClient.cpp:508-530`).
    fn send_heartbeat_if_necessary(&mut self) -> bool {
        if !self.socket_connected {
            return true;
        }
        // C++ `constexpr heatbeatInterval = 45s` (note the C++ typo, `:511`),
        // with the C++'s nested-if shape collapsed (identical semantics).
        if self.clock.now() - self.last_heartbeat_time > Duration::from_secs(45)
            && !self.send_heartbeat()
        {
            self.disconnect();
            return false;
        }
        true
    }

    /// C++ `receiveDataFromServer(p)` (`HexagonClient.cpp:532-775`).
    fn receive_data_from_server(&mut self, p: &mut Packet) -> bool {
        if !self.socket_connected {
            return false;
        }
        if !self.recv_packet(p) {
            return false;
        }

        // C++ `_errorOss.str("")` (`:541`) — cleared before the single use.
        self.error_oss.clear();
        let rt_key = self.client_rt_keys.as_ref().map(|k| &k.key_receive);
        let pv = decode_server_to_client_packet(rt_key, p, &mut self.error_oss);

        match pv {
            // C++ `PInvalid` arm (`:547-551`).
            ServerToClient::PInvalid { .. } => false,
            // C++ `PEncryptedMsg` arm (`:553-556`) — a still-encrypted
            // message means the RT keys are missing/stale.
            ServerToClient::PEncryptedMsg { .. } => false,
            // C++ `STCPKick` arm (`:558-565`): event + disconnect, `true`.
            ServerToClient::STCPKick => {
                self.add_event(Event::Kicked);
                self.disconnect();
                true
            }
            // C++ `STCPPublicKey` arm (`:567-604`).
            ServerToClient::STCPPublicKey { key } => {
                self.server_public_key = Some(key);
                let rt_keys = calculate_client_session_sodium_rt_keys(&self.client_ps_keys, &key);
                let Some(rt_keys) = rt_keys else {
                    // C++ `:592-598`: "Failed calculating RT keys" + disconnect.
                    self.disconnect();
                    return false;
                };
                self.client_rt_keys = Some(rt_keys);
                true
            }
            // C++ `STCPRegistrationSuccess` arm (`:606-611`).
            ServerToClient::STCPRegistrationSuccess => {
                self.add_event(Event::RegistrationSuccess);
                true
            }
            // C++ `STCPRegistrationFailure` arm (`:613-619`).
            ServerToClient::STCPRegistrationFailure { error } => {
                self.add_event(Event::RegistrationFailure { error });
                true
            }
            // C++ `STCPLoginSuccess` arm (`:621-639`): store the token +
            // name, `LoggedIn`, then immediately request the server status.
            ServerToClient::STCPLoginSuccess {
                login_token,
                login_name,
            } => {
                self.login_token = Some(login_token);
                self.login_name = Some(login_name);
                self.state = State::LoggedIn;
                self.send_request_server_status(login_token)
            }
            // C++ `STCPLoginFailure` arm (`:641-648`).
            ServerToClient::STCPLoginFailure { error } => {
                self.add_event(Event::LoginFailure { error });
                true
            }
            // C++ `STCPLogoutSuccess` arm (`:650-655`).
            ServerToClient::STCPLogoutSuccess => {
                self.add_event(Event::LogoutSuccess);
                true
            }
            // C++ `STCPLogoutFailure` arm (`:657-662`).
            ServerToClient::STCPLogoutFailure => {
                self.add_event(Event::LogoutFailure);
                true
            }
            // C++ `STCPDeleteAccountSuccess` arm (`:664-669`).
            ServerToClient::STCPDeleteAccountSuccess => {
                self.add_event(Event::DeleteAccountSuccess);
                true
            }
            // C++ `STCPDeleteAccountFailure` arm (`:671-678`).
            ServerToClient::STCPDeleteAccountFailure { error } => {
                self.add_event(Event::DeleteAccountFailure { error });
                true
            }
            // C++ `STCPTopScores` arm (`:680-689`).
            ServerToClient::STCPTopScores {
                level_validator,
                scores,
            } => {
                self.add_event(Event::ReceivedTopScores {
                    level_validator,
                    scores,
                });
                true
            }
            // C++ `STCPOwnScore` arm (`:691-698`).
            ServerToClient::STCPOwnScore {
                level_validator,
                score,
            } => {
                self.add_event(Event::ReceivedOwnScore {
                    level_validator,
                    score,
                });
                true
            }
            // C++ `STCPTopScoresAndOwnScore` arm (`:700-715`): the top-scores
            // event always, the own-score event only when present.
            ServerToClient::STCPTopScoresAndOwnScore {
                level_validator,
                scores,
                own_score,
            } => {
                self.add_event(Event::ReceivedTopScores {
                    level_validator: level_validator.clone(),
                    scores,
                });
                if let Some(score) = own_score {
                    self.add_event(Event::ReceivedOwnScore {
                        level_validator,
                        score,
                    });
                }
                true
            }
            // C++ `STCPServerStatus` arm (`:717-772`).
            ServerToClient::STCPServerStatus {
                protocol_version,
                game_version,
                supported_level_validators,
            } => {
                // C++ `:728-731`: a game-version mismatch is an event ONLY
                // (the session proceeds — the game will refuse to play, but
                // the client keeps talking).
                if game_version != GAME_VERSION {
                    self.add_event(Event::GameVersionMismatch);
                }
                // C++ `:733-738`: a protocol mismatch is FATAL — event +
                // disconnect, `true`.
                if protocol_version != PROTOCOL_VERSION {
                    self.add_event(Event::ProtocolVersionMismatch);
                    self.disconnect();
                    return true;
                }
                // C++ `:740-743`: the validators are INSERTED (union with
                // any previously-seen set — the C++ never clears it).
                for lv in supported_level_validators {
                    self.level_validators_supported_by_server.insert(lv);
                }
                self.state = State::LoggedInReady;
                self.add_event(Event::LoginSuccess);
                // C++ `:749-771`: assert the token, then `sendReady`.
                self.send_ready(
                    self.login_token
                        .expect("STCPLoginSuccess set the token before the status"),
                )
            }
        }
    }

    /// C++ `update()` (`HexagonClient.cpp:777-794`): the per-frame driver —
    /// the heartbeat keep-alive, then one receive (the C++ `try/catch`
    /// around `ssvu` exceptions has no Rust analogue: the ported methods
    /// return `bool` and cannot throw).
    pub fn update(&mut self) {
        if !self.socket_connected {
            return;
        }
        self.send_heartbeat_if_necessary();
        // C++ `receiveDataFromServer(_packetBuffer)` (`:791`). The
        // `mem::replace` sidesteps the `&mut self` + field double borrow;
        // the buffer goes back (the C++ member holds the last decoded
        // packet's bytes with the read position at the end post-update —
        // nothing reads them before the next `reset()`).
        let mut buf = std::mem::replace(&mut self.packet_buffer, Packet::new());
        self.receive_data_from_server(&mut buf);
        self.packet_buffer = buf;
    }

    /// C++ `tryRegister(name, password)` (`HexagonClient.cpp:813-830`).
    pub fn try_register(&mut self, name: &str, password: &str) -> bool {
        if !self.connected_and_in_state(State::Connected) {
            return false;
        }
        // C++ `name.empty() || name.size() > 32 || password.empty()` —
        // `size()` is the byte count, as is `str::len()`.
        if name.is_empty() || name.len() > 32 || password.is_empty() {
            self.add_event(Event::RegistrationFailure {
                error: "Name or password fields too long or empty".to_string(),
            });
            return false;
        }
        // C++ `SSVOH_ASSERT(_ticketSteamID.has_value())` — the constructor
        // guarantees it (or the client is in `InitError`/`ConnectionError`,
        // where `connected_and_in_state` is already false).
        self.send_register(
            self.ticket_steam_id.expect("ticket set by the constructor"),
            name,
            &salt_and_hash_pwd(password),
        )
    }

    /// C++ `tryLogin(name, password)` (`HexagonClient.cpp:832-848`).
    pub fn try_login(&mut self, name: &str, password: &str) -> bool {
        if !self.connected_and_in_state(State::Connected) {
            return false;
        }
        if name.is_empty() || name.len() > 32 || password.is_empty() {
            self.add_event(Event::LoginFailure {
                error: "Name or password fields too long or empty".to_string(),
            });
            return false;
        }
        self.send_login(
            self.ticket_steam_id.expect("ticket set by the constructor"),
            name,
            &salt_and_hash_pwd(password),
        )
    }

    /// C++ `tryLogoutFromServer()` (`HexagonClient.cpp:850-863`): the state
    /// drops to `Connected` and the session token/name are cleared BEFORE
    /// the logout packet goes out.
    pub fn try_logout_from_server(&mut self) -> bool {
        if !self.connected_and_in_any_state(State::LoggedIn, State::LoggedInReady) {
            return false;
        }
        self.state = State::Connected;
        self.login_token = None;
        self.login_name = None;
        self.send_logout(self.ticket_steam_id.expect("ticket set by the constructor"))
    }

    /// C++ `tryDeleteAccount(password)` (`HexagonClient.cpp:865-874`).
    pub fn try_delete_account(&mut self, password: &str) -> bool {
        if !self.connected_and_in_state(State::Connected) {
            return false;
        }
        self.send_delete_account(
            self.ticket_steam_id.expect("ticket set by the constructor"),
            &salt_and_hash_pwd(password),
        )
    }

    /// C++ `tryRequestTopScores(levelValidator)`
    /// (`HexagonClient.cpp:876-884`).
    pub fn try_request_top_scores(&mut self, level_validator: &str) -> bool {
        if !self.connected_and_in_state(State::LoggedInReady) {
            return false;
        }
        self.send_request_top_scores(
            self.login_token
                .expect("LoggedInReady implies a login token"),
            level_validator,
        )
    }

    /// C++ `trySendCompressedReplay(levelValidator, compressedReplayFile)`
    /// (`HexagonClient.cpp:887-906`): an unsupported level is a `true`
    /// no-op (the replay is simply not sent — NOT a failure).
    pub fn try_send_compressed_replay(
        &mut self,
        level_validator: &str,
        compressed_replay_file: &oh_replay::CompressedReplayFile,
    ) -> bool {
        if !self.connected_and_in_state(State::LoggedInReady) {
            return false;
        }
        if !self.is_level_supported_by_server(level_validator) {
            return true;
        }
        self.send_compressed_replay(
            self.login_token
                .expect("LoggedInReady implies a login token"),
            level_validator,
            compressed_replay_file,
        )
    }

    /// C++ `tryRequestOwnScore(levelValidator)`
    /// (`HexagonClient.cpp:908-916`).
    pub fn try_request_own_score(&mut self, level_validator: &str) -> bool {
        if !self.connected_and_in_state(State::LoggedInReady) {
            return false;
        }
        self.send_request_own_score(
            self.login_token
                .expect("LoggedInReady implies a login token"),
            level_validator,
        )
    }

    /// C++ `tryRequestTopScoresAndOwnScore(levelValidator)`
    /// (`HexagonClient.cpp:919-927`).
    pub fn try_request_top_scores_and_own_score(&mut self, level_validator: &str) -> bool {
        if !self.connected_and_in_state(State::LoggedInReady) {
            return false;
        }
        self.send_request_top_scores_and_own_score(
            self.login_token
                .expect("LoggedInReady implies a login token"),
            level_validator,
        )
    }

    /// C++ `trySendStartedGame(levelValidator)`
    /// (`HexagonClient.cpp:931-939`).
    pub fn try_send_started_game(&mut self, level_validator: &str) -> bool {
        if !self.connected_and_in_state(State::LoggedInReady) {
            return false;
        }
        self.send_started_game(
            self.login_token
                .expect("LoggedInReady implies a login token"),
            level_validator,
        )
    }

    /// C++ `getState()` (`HexagonClient.cpp:942-944`).
    #[must_use]
    pub fn get_state(&self) -> State {
        self.state
    }

    /// C++ `hasRTKeys()` (`HexagonClient.cpp:947-949`).
    #[must_use]
    pub fn has_rt_keys(&self) -> bool {
        self.client_rt_keys.is_some()
    }

    /// C++ `getLoginName()` (`HexagonClient.cpp:953-955`).
    #[must_use]
    pub fn get_login_name(&self) -> Option<&String> {
        self.login_name.as_ref()
    }

    /// C++ `pollEvent()` (`HexagonClient.cpp:975-984`): pop the front event.
    pub fn poll_event(&mut self) -> Option<Event> {
        self.events.pop_front()
    }

    /// C++ `isLevelSupportedByServer(levelValidator)`
    /// (`HexagonClient.cpp:986-988`).
    #[must_use]
    pub fn is_level_supported_by_server(&self, level_validator: &str) -> bool {
        self.level_validators_supported_by_server
            .contains(level_validator)
    }
}

impl Drop for HexagonClient {
    /// C++ `~HexagonClient()` (`HexagonClient.cpp:475-479`): disconnect on
    /// destruction.
    fn drop(&mut self) {
        self.disconnect();
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::GameVersion;
    use crate::shared::{make_server_to_client_encrypted_packet, make_server_to_client_packet};
    use crate::sodium::{
        calculate_server_session_sodium_rt_keys, SodiumPsKeys, SodiumPublicKeyArray,
    };
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::rc::Rc;

    /// A scripted raw socket: `connect_results` / `send_results` /
    /// `recv_results` override the default behaviour (success / send-all /
    /// serve `net`); every byte sent is recorded in `sent`; `net` is what
    /// `recv` serves (up to `recv_chunk` bytes per raw call — the SFML
    /// framing loop then does its chunking on top).
    struct FakeSocket {
        connected: bool,
        connect_results: VecDeque<RecvResult>,
        send_results: VecDeque<SendResult>,
        recv_results: VecDeque<RecvResult>,
        net: Vec<u8>,
        net_pos: usize,
        recv_chunk: usize,
        sent: Vec<u8>,
        disconnects: usize,
    }

    impl FakeSocket {
        fn new() -> Self {
            Self {
                connected: false,
                connect_results: VecDeque::new(),
                send_results: VecDeque::new(),
                recv_results: VecDeque::new(),
                net: Vec::new(),
                net_pos: 0,
                recv_chunk: 1024,
                sent: Vec::new(),
                disconnects: 0,
            }
        }
        fn push_net(&mut self, bytes: &[u8]) {
            self.net.extend_from_slice(bytes);
        }
        /// The complete frames written so far, deframed (u32-BE length +
        /// payload) — the inverse of [`frame`].
        fn deframed(&self) -> Vec<Vec<u8>> {
            let mut out = Vec::new();
            let mut pos = 0;
            while pos + 4 <= self.sent.len() {
                let len = u32::from_be_bytes(self.sent[pos..pos + 4].try_into().unwrap()) as usize;
                pos += 4;
                out.push(self.sent[pos..pos + len].to_vec());
                pos += len;
            }
            out
        }
    }

    impl NetSocket for FakeSocket {
        fn connect(&mut self, _addr: SocketAddr, _timeout: Duration) -> RecvResult {
            if let Some(r) = self.connect_results.pop_front() {
                self.connected = matches!(r, RecvResult::Done(_));
                return r;
            }
            self.connected = true;
            RecvResult::Done(0)
        }

        fn send(&mut self, buf: &[u8]) -> SendResult {
            if !self.connected {
                return SendResult::Error;
            }
            if let Some(r) = self.send_results.pop_front() {
                if matches!(r, SendResult::Done) {
                    self.sent.extend_from_slice(buf);
                }
                return r;
            }
            self.sent.extend_from_slice(buf);
            SendResult::Done
        }

        fn recv(&mut self, buf: &mut [u8]) -> RecvResult {
            if let Some(r) = self.recv_results.pop_front() {
                return r;
            }
            if self.net_pos >= self.net.len() {
                return RecvResult::NotReady;
            }
            let n = (self.net.len() - self.net_pos)
                .min(buf.len())
                .min(self.recv_chunk);
            buf[..n].copy_from_slice(&self.net[self.net_pos..self.net_pos + n]);
            self.net_pos += n;
            RecvResult::Done(n)
        }

        fn disconnect(&mut self) {
            self.connected = false;
            self.disconnects += 1;
        }
    }

    /// `Rc<RefCell<FakeSocket>>` behind the `Box<dyn NetSocket>` so the test
    /// thread can script/inspect the same socket the client drives (the
    /// client is single-threaded by the D-014 contract correction, so
    /// `RefCell` is sound).
    struct SharedFake(Rc<RefCell<FakeSocket>>);

    impl NetSocket for SharedFake {
        fn connect(&mut self, addr: SocketAddr, timeout: Duration) -> RecvResult {
            self.0.borrow_mut().connect(addr, timeout)
        }
        fn send(&mut self, buf: &[u8]) -> SendResult {
            self.0.borrow_mut().send(buf)
        }
        fn recv(&mut self, buf: &mut [u8]) -> RecvResult {
            self.0.borrow_mut().recv(buf)
        }
        fn disconnect(&mut self) {
            self.0.borrow_mut().disconnect();
        }
    }

    /// A scripted Steam manager: the ticket response arrives after
    /// `response_after` `run_callbacks` calls.
    struct FakeSteam {
        response_after: u32,
        valid_ticket: bool,
        steam_id: Option<u64>,
        callbacks_run: u32,
    }

    impl FakeSteam {
        fn immediate(steam_id: u64) -> Self {
            Self {
                response_after: 0,
                valid_ticket: true,
                steam_id: Some(steam_id),
                callbacks_run: 0,
            }
        }
    }

    impl SteamManager for FakeSteam {
        fn got_encrypted_app_ticket_response(&self) -> bool {
            self.callbacks_run >= self.response_after
        }
        fn run_callbacks(&mut self) {
            self.callbacks_run += 1;
        }
        fn got_encrypted_app_ticket(&self) -> bool {
            self.valid_ticket
        }
        fn get_ticket_steam_id(&self) -> Option<u64> {
            self.steam_id
        }
    }

    fn addr() -> SocketAddr {
        SocketAddr::from(([127, 0, 0, 1], 12345))
    }

    /// A connected client over a shared fake socket (the C++ constructor
    /// ran: ticket OK + `connect()`).
    fn connected_client(steam_id: u64) -> (HexagonClient, Rc<RefCell<FakeSocket>>) {
        let fake = Rc::new(RefCell::new(FakeSocket::new()));
        let client = HexagonClient::with_socket(
            Box::new(FakeSteam::immediate(steam_id)),
            addr(),
            Box::new(SharedFake(fake.clone())),
            ClientClock::new(),
        );
        (client, fake)
    }

    /// Frame a server packet the way the C++ server does (u32-BE length +
    /// the `makeServerToClientPacket` bytes — the fork `send(Packet&)`
    /// framing, `TcpSocket.cpp:309-365`).
    fn frame(packet_bytes: &[u8]) -> Vec<u8> {
        let mut v = Vec::with_capacity(4 + packet_bytes.len());
        v.extend_from_slice(&(packet_bytes.len() as u32).to_be_bytes());
        v.extend_from_slice(packet_bytes);
        v
    }

    fn plain_packet(p: &ServerToClient) -> Vec<u8> {
        let mut pkt = Packet::new();
        make_server_to_client_packet(&mut pkt, p);
        pkt.data().to_vec()
    }

    /// The client's PS public key, as sent in the constructor's
    /// `sendPublicKey` (frame 2: 6-byte header + type 4 + 32 key bytes).
    fn client_public_key(fake: &Rc<RefCell<FakeSocket>>) -> SodiumPublicKeyArray {
        let frames = fake.borrow().deframed();
        frames[1][7..39].try_into().unwrap()
    }

    /// Feed a PLAIN server packet (the C++ server sends only STCPKick and
    /// STCPPublicKey in the clear, `HexagonServer.cpp:154-164`).
    fn feed_plain(client: &mut HexagonClient, fake: &Rc<RefCell<FakeSocket>>, p: &ServerToClient) {
        fake.borrow_mut().push_net(&frame(&plain_packet(p)));
        client.update();
    }

    /// Feed an ENCRYPTED server packet (every other STCP — the
    /// `sendEncrypted` envelope, `HexagonServer.cpp:135-150`) — exercising
    /// the client's `key_receive` decryption path.
    fn feed_encrypted(
        client: &mut HexagonClient,
        fake: &Rc<RefCell<FakeSocket>>,
        server_ps: &SodiumPsKeys,
        p: &ServerToClient,
    ) {
        // The server's RT keys are derived from (server PS keys, client
        // public key) — `calculateServerSessionSodiumRTKeys`.
        let rt =
            calculate_server_session_sodium_rt_keys(server_ps, &client_public_key(fake)).unwrap();
        let mut pkt = Packet::new();
        make_server_to_client_encrypted_packet(&rt.key_transmit, &mut pkt, p);
        fake.borrow_mut().push_net(&frame(pkt.data()));
        client.update();
    }

    fn ps(position: u32, user: &str, value: f64) -> crate::db_records::ProcessedScore {
        crate::db_records::ProcessedScore {
            position,
            user_name: user.to_string(),
            score_timestamp: 0,
            score_value: value,
        }
    }

    /// Drive a client to `LoggedInReady` (key exchange + login + status).
    fn to_ready(
        client: &mut HexagonClient,
        fake: &Rc<RefCell<FakeSocket>>,
        server_ps: &SodiumPsKeys,
    ) {
        feed_plain(
            client,
            fake,
            &ServerToClient::STCPPublicKey {
                key: server_ps.key_public,
            },
        );
        feed_encrypted(
            client,
            fake,
            server_ps,
            &ServerToClient::STCPLoginSuccess {
                login_token: 9,
                login_name: "gary".into(),
            },
        );
        feed_encrypted(
            client,
            fake,
            server_ps,
            &ServerToClient::STCPServerStatus {
                protocol_version: PROTOCOL_VERSION,
                game_version: GAME_VERSION,
                supported_level_validators: vec!["lv1".into(), "lv2".into()],
            },
        );
    }

    // -- constructor / connection -------------------------------------------

    #[test]
    fn constructor_connects_and_sends_heartbeat_and_public_key() {
        let (mut client, fake) = connected_client(777);
        assert_eq!(client.get_state(), State::Connected);
        assert_eq!(client.poll_event(), Some(Event::ConnectionSuccess));
        assert!(client.poll_event().is_none());

        let frames = fake.borrow().deframed();
        assert_eq!(frames.len(), 2, "the connect flow sent exactly 2 frames");
        // Heartbeat: 6-byte header ('o','h', proto, major, minor, micro —
        // `Shared.cpp:92-109`) + type byte 2 (CTSPHeartbeat).
        assert_eq!(frames[0], vec![b'o', b'h', 0, 2, 1, 7, 2]);
        // Public key: 6-byte header + type 4 + 32 key bytes.
        assert_eq!(frames[1].len(), 6 + 1 + 32);
        assert_eq!(&frames[1][6..7], &[4]);

        assert!(
            !client.has_rt_keys(),
            "no RT keys until the server's key arrives"
        );
    }

    #[test]
    fn ticket_poll_failure_is_init_error() {
        // Never responds: the C++ polls 16 times (tries > 15) then fails —
        // no connect, no frames.
        let fake = Rc::new(RefCell::new(FakeSocket::new()));
        let client = HexagonClient::with_socket(
            Box::new(FakeSteam {
                response_after: u32::MAX,
                valid_ticket: true,
                steam_id: Some(1),
                callbacks_run: 0,
            }),
            addr(),
            Box::new(SharedFake(fake.clone())),
            ClientClock::custom(Instant::now, |_| {} /* no real 50 ms sleep */),
        );
        assert_eq!(client.get_state(), State::InitError);
        assert!(
            fake.borrow().sent.is_empty(),
            "nothing sent before a ticket"
        );
    }

    #[test]
    fn ticket_present_but_no_steam_id_is_init_error() {
        let fake = Rc::new(RefCell::new(FakeSocket::new()));
        let client = HexagonClient::with_socket(
            Box::new(FakeSteam {
                response_after: 0,
                valid_ticket: true,
                steam_id: None,
                callbacks_run: 0,
            }),
            addr(),
            Box::new(SharedFake(fake.clone())),
            ClientClock::custom(Instant::now, |_| {}),
        );
        assert_eq!(client.get_state(), State::InitError);
    }

    #[test]
    fn connect_failure_yields_connection_error_event() {
        let fake = Rc::new(RefCell::new(FakeSocket::new()));
        fake.borrow_mut()
            .connect_results
            .push_back(RecvResult::Error);
        let mut client = HexagonClient::with_socket(
            Box::new(FakeSteam::immediate(7)),
            addr(),
            Box::new(SharedFake(fake.clone())),
            ClientClock::custom(Instant::now, |_| {}),
        );
        assert_eq!(client.get_state(), State::ConnectionError);
        assert_eq!(
            client.poll_event(),
            Some(Event::ConnectionFailure {
                error: "Failure connecting, error initializing TCP socket".into(),
            })
        );
    }

    #[test]
    fn update_is_noop_without_socket() {
        let fake = Rc::new(RefCell::new(FakeSocket::new()));
        fake.borrow_mut()
            .connect_results
            .push_back(RecvResult::Error);
        let mut client = HexagonClient::with_socket(
            Box::new(FakeSteam::immediate(7)),
            addr(),
            Box::new(SharedFake(fake.clone())),
            ClientClock::custom(Instant::now, |_| {}),
        );
        client.update(); // must be a no-op (HexagonClient.cpp:779-782)
        assert!(fake.borrow().sent.is_empty());
    }

    #[test]
    fn partial_sends_exhaust_the_five_retries() {
        let fake = Rc::new(RefCell::new(FakeSocket::new()));
        // 6 scripted Partials: sends 0..5 recurse (each Partial(0) makes no
        // progress), the 7th call (tries == 6 > 5) fails — the C++
        // `sendPacketRecursive` gives up (`HexagonClient.cpp:119-122`).
        for _ in 0..6 {
            fake.borrow_mut()
                .send_results
                .push_back(SendResult::Partial(0));
        }
        let mut client = HexagonClient::with_socket(
            Box::new(FakeSteam::immediate(7)),
            addr(),
            Box::new(SharedFake(fake.clone())),
            ClientClock::custom(Instant::now, |_| {}),
        );
        assert_eq!(client.get_state(), State::ConnectionError);
        assert_eq!(
            client.poll_event(),
            Some(Event::ConnectionFailure {
                error: "Failure connecting, error sending first heartbeat".into(),
            })
        );
    }

    // -- handshake / session -------------------------------------------------

    #[test]
    fn server_public_key_sets_rt_keys() {
        let (mut client, fake) = connected_client(7);
        let server_ps = generate_sodium_ps_keys();
        feed_plain(
            &mut client,
            &fake,
            &ServerToClient::STCPPublicKey {
                key: server_ps.key_public,
            },
        );
        assert!(
            client.has_rt_keys(),
            "RT keys calculated from the server key"
        );
        assert_eq!(
            client.get_state(),
            State::Connected,
            "the key exchange does not change the state"
        );
    }

    #[test]
    fn login_success_stores_session_and_requests_status() {
        let (mut client, fake) = connected_client(7);
        let server_ps = generate_sodium_ps_keys();
        feed_plain(
            &mut client,
            &fake,
            &ServerToClient::STCPPublicKey {
                key: server_ps.key_public,
            },
        );

        // The encrypted STCPLoginSuccess decrypts only if the client's
        // `key_receive` is right — this pins the full RT round trip.
        feed_encrypted(
            &mut client,
            &fake,
            &server_ps,
            &ServerToClient::STCPLoginSuccess {
                login_token: 42,
                login_name: "gary".into(),
            },
        );

        assert_eq!(client.get_state(), State::LoggedIn);
        assert_eq!(client.get_login_name(), Some(&"gary".to_string()));
        // The login handler immediately requests the server status
        // (`HexagonClient.cpp:638`) — an encrypted CTSPRequestServerStatus.
        let frames = fake.borrow().deframed();
        assert_eq!(
            &frames.last().unwrap()[6..7],
            &[1],
            "an encrypted envelope (PEncryptedMsg, type 1)"
        );
    }

    #[test]
    fn server_status_moves_to_ready_and_sends_ready() {
        let (mut client, fake) = connected_client(7);
        let server_ps = generate_sodium_ps_keys();
        to_ready(&mut client, &fake, &server_ps);

        assert_eq!(client.get_state(), State::LoggedInReady);
        assert_eq!(client.poll_event(), Some(Event::ConnectionSuccess));
        assert_eq!(client.poll_event(), Some(Event::LoginSuccess));
        assert!(client.is_level_supported_by_server("lv1"));
        assert!(client.is_level_supported_by_server("lv2"));
        assert!(!client.is_level_supported_by_server("lv3"));
        // The final frame is the encrypted CTSPReady (type 16 inside).
        let frames = fake.borrow().deframed();
        assert_eq!(&frames.last().unwrap()[6..7], &[1]);
    }

    #[test]
    fn server_status_validators_union_across_statuses() {
        let (mut client, fake) = connected_client(7);
        let server_ps = generate_sodium_ps_keys();
        to_ready(&mut client, &fake, &server_ps);
        // A second status ADDS (the C++ `insert`, `HexagonClient.cpp:740-743`).
        feed_encrypted(
            &mut client,
            &fake,
            &server_ps,
            &ServerToClient::STCPServerStatus {
                protocol_version: PROTOCOL_VERSION,
                game_version: GAME_VERSION,
                supported_level_validators: vec!["lv3".into()],
            },
        );
        assert!(
            client.is_level_supported_by_server("lv1"),
            "old validators kept"
        );
        assert!(
            client.is_level_supported_by_server("lv3"),
            "new validators added"
        );
    }

    #[test]
    fn game_version_mismatch_is_event_only() {
        let (mut client, fake) = connected_client(7);
        let server_ps = generate_sodium_ps_keys();
        feed_plain(
            &mut client,
            &fake,
            &ServerToClient::STCPPublicKey {
                key: server_ps.key_public,
            },
        );
        feed_encrypted(
            &mut client,
            &fake,
            &server_ps,
            &ServerToClient::STCPLoginSuccess {
                login_token: 1,
                login_name: "n".into(),
            },
        );
        feed_encrypted(
            &mut client,
            &fake,
            &server_ps,
            &ServerToClient::STCPServerStatus {
                protocol_version: PROTOCOL_VERSION,
                game_version: GameVersion::new(9, 9, 9),
                supported_level_validators: vec![],
            },
        );
        // The session proceeds (HexagonClient.cpp:728-731).
        assert_eq!(client.get_state(), State::LoggedInReady);
        // Drain the constructor's EConnectionSuccess, then the mismatch.
        assert_eq!(client.poll_event(), Some(Event::ConnectionSuccess));
        assert_eq!(client.poll_event(), Some(Event::GameVersionMismatch));
        assert_eq!(client.poll_event(), Some(Event::LoginSuccess));
    }

    #[test]
    fn protocol_version_mismatch_disconnects() {
        let (mut client, fake) = connected_client(7);
        let server_ps = generate_sodium_ps_keys();
        feed_plain(
            &mut client,
            &fake,
            &ServerToClient::STCPPublicKey {
                key: server_ps.key_public,
            },
        );
        feed_encrypted(
            &mut client,
            &fake,
            &server_ps,
            &ServerToClient::STCPLoginSuccess {
                login_token: 1,
                login_name: "n".into(),
            },
        );
        feed_encrypted(
            &mut client,
            &fake,
            &server_ps,
            &ServerToClient::STCPServerStatus {
                protocol_version: 99,
                game_version: GAME_VERSION,
                supported_level_validators: vec![],
            },
        );
        assert_eq!(client.get_state(), State::Disconnected);
        assert_eq!(client.poll_event(), Some(Event::ConnectionSuccess));
        assert_eq!(client.poll_event(), Some(Event::ProtocolVersionMismatch));
        assert!(fake.borrow().disconnects >= 1, "the socket was closed");
    }

    #[test]
    fn kick_disconnects_and_events() {
        let (mut client, fake) = connected_client(7);
        feed_plain(&mut client, &fake, &ServerToClient::STCPKick);
        assert_eq!(client.get_state(), State::Disconnected);
        assert_eq!(client.poll_event(), Some(Event::ConnectionSuccess));
        assert_eq!(client.poll_event(), Some(Event::Kicked));
        assert!(fake.borrow().disconnects >= 1);
    }

    #[test]
    fn registration_events() {
        let (mut client, fake) = connected_client(7);
        let server_ps = generate_sodium_ps_keys();
        feed_plain(
            &mut client,
            &fake,
            &ServerToClient::STCPPublicKey {
                key: server_ps.key_public,
            },
        );
        feed_encrypted(
            &mut client,
            &fake,
            &server_ps,
            &ServerToClient::STCPRegistrationFailure {
                error: "name taken".into(),
            },
        );
        feed_encrypted(
            &mut client,
            &fake,
            &server_ps,
            &ServerToClient::STCPRegistrationSuccess,
        );
        assert_eq!(client.poll_event(), Some(Event::ConnectionSuccess));
        assert_eq!(
            client.poll_event(),
            Some(Event::RegistrationFailure {
                error: "name taken".into(),
            })
        );
        assert_eq!(client.poll_event(), Some(Event::RegistrationSuccess));
    }

    #[test]
    fn logout_and_delete_account_events() {
        let (mut client, fake) = connected_client(7);
        let server_ps = generate_sodium_ps_keys();
        feed_plain(
            &mut client,
            &fake,
            &ServerToClient::STCPPublicKey {
                key: server_ps.key_public,
            },
        );
        feed_encrypted(
            &mut client,
            &fake,
            &server_ps,
            &ServerToClient::STCPLogoutFailure,
        );
        feed_encrypted(
            &mut client,
            &fake,
            &server_ps,
            &ServerToClient::STCPDeleteAccountFailure {
                error: "bad hash".into(),
            },
        );
        feed_encrypted(
            &mut client,
            &fake,
            &server_ps,
            &ServerToClient::STCPDeleteAccountSuccess,
        );
        assert_eq!(client.poll_event(), Some(Event::ConnectionSuccess));
        assert_eq!(client.poll_event(), Some(Event::LogoutFailure));
        assert_eq!(
            client.poll_event(),
            Some(Event::DeleteAccountFailure {
                error: "bad hash".into(),
            })
        );
        assert_eq!(client.poll_event(), Some(Event::DeleteAccountSuccess));
    }

    #[test]
    fn score_events_round_trip() {
        let (mut client, fake) = connected_client(7);
        let server_ps = generate_sodium_ps_keys();
        feed_plain(
            &mut client,
            &fake,
            &ServerToClient::STCPPublicKey {
                key: server_ps.key_public,
            },
        );
        feed_encrypted(
            &mut client,
            &fake,
            &server_ps,
            &ServerToClient::STCPTopScores {
                level_validator: "lv".into(),
                scores: vec![ps(0, "a", 1.0)],
            },
        );
        feed_encrypted(
            &mut client,
            &fake,
            &server_ps,
            &ServerToClient::STCPOwnScore {
                level_validator: "lv".into(),
                score: ps(3, "me", 0.5),
            },
        );
        feed_encrypted(
            &mut client,
            &fake,
            &server_ps,
            &ServerToClient::STCPTopScoresAndOwnScore {
                level_validator: "lv2".into(),
                scores: vec![ps(0, "b", 2.0)],
                own_score: Some(ps(1, "me", 1.5)),
            },
        );
        feed_encrypted(
            &mut client,
            &fake,
            &server_ps,
            &ServerToClient::STCPTopScoresAndOwnScore {
                level_validator: "lv3".into(),
                scores: vec![],
                own_score: None, // the C++ skips the own-score event
            },
        );

        assert_eq!(client.poll_event(), Some(Event::ConnectionSuccess));
        assert_eq!(
            client.poll_event(),
            Some(Event::ReceivedTopScores {
                level_validator: "lv".into(),
                scores: vec![ps(0, "a", 1.0)],
            })
        );
        assert_eq!(
            client.poll_event(),
            Some(Event::ReceivedOwnScore {
                level_validator: "lv".into(),
                score: ps(3, "me", 0.5),
            })
        );
        assert_eq!(
            client.poll_event(),
            Some(Event::ReceivedTopScores {
                level_validator: "lv2".into(),
                scores: vec![ps(0, "b", 2.0)],
            })
        );
        assert_eq!(
            client.poll_event(),
            Some(Event::ReceivedOwnScore {
                level_validator: "lv2".into(),
                score: ps(1, "me", 1.5),
            })
        );
        assert_eq!(
            client.poll_event(),
            Some(Event::ReceivedTopScores {
                level_validator: "lv3".into(),
                scores: vec![],
            })
        );
        assert!(
            client.poll_event().is_none(),
            "no own-score event when ownScore is absent"
        );
    }

    // -- the try_* API --------------------------------------------------------

    #[test]
    fn try_register_validates_fields() {
        let (mut client, fake) = connected_client(7);
        assert!(!client.try_register("", "pw"), "empty name refused");
        assert_eq!(client.poll_event(), Some(Event::ConnectionSuccess));
        assert_eq!(
            client.poll_event(),
            Some(Event::RegistrationFailure {
                error: "Name or password fields too long or empty".into(),
            })
        );
        assert!(
            !client.try_register(&"a".repeat(33), "pw"),
            "33-byte name refused"
        );
        assert!(!client.try_register("ok", ""), "empty password refused");
        // A valid name reaches send_encrypted → fails on the missing RT
        // keys (no crash) and nothing new is recorded on the wire.
        assert!(!client.try_register("ok", "pw"));
        assert_eq!(fake.borrow().deframed().len(), 2, "only the connect frames");
    }

    #[test]
    fn try_register_sends_the_hashed_password() {
        let (mut client, fake) = connected_client(7);
        let server_ps = generate_sodium_ps_keys();
        feed_plain(
            &mut client,
            &fake,
            &ServerToClient::STCPPublicKey {
                key: server_ps.key_public,
            },
        );
        let before = fake.borrow().sent.len();
        assert!(client.try_register("gary", "hunter2"));
        assert!(fake.borrow().sent.len() > before, "the register went out");
        // The envelope is a PEncryptedMsg (type 1).
        assert_eq!(&fake.borrow().deframed().last().unwrap()[6..7], &[1]);
    }

    #[test]
    fn try_login_flow_and_logout_clears_session() {
        let (mut client, fake) = connected_client(7);
        let server_ps = generate_sodium_ps_keys();
        feed_plain(
            &mut client,
            &fake,
            &ServerToClient::STCPPublicKey {
                key: server_ps.key_public,
            },
        );

        assert!(!client.try_login("", "x"));
        assert_eq!(client.poll_event(), Some(Event::ConnectionSuccess));
        assert_eq!(
            client.poll_event(),
            Some(Event::LoginFailure {
                error: "Name or password fields too long or empty".into(),
            })
        );
        assert!(client.try_login("gary", "pw"), "login sent");

        // Logout from `Connected` is refused (need LoggedIn*).
        assert!(!client.try_logout_from_server());

        to_ready_logged_in(&mut client, &fake, &server_ps);
        assert_eq!(client.get_state(), State::LoggedInReady);

        // Logout: state → Connected, token + name cleared, encrypted packet
        // sent (`HexagonClient.cpp:850-863`).
        assert!(client.try_logout_from_server());
        assert_eq!(client.get_state(), State::Connected);
        assert_eq!(client.get_login_name(), None);
        assert_eq!(&fake.borrow().deframed().last().unwrap()[6..7], &[1]);
    }

    /// The login-success + status pair (no key exchange — the caller did it).
    fn to_ready_logged_in(
        client: &mut HexagonClient,
        fake: &Rc<RefCell<FakeSocket>>,
        server_ps: &SodiumPsKeys,
    ) {
        feed_encrypted(
            client,
            fake,
            server_ps,
            &ServerToClient::STCPLoginSuccess {
                login_token: 5,
                login_name: "gary".into(),
            },
        );
        feed_encrypted(
            client,
            fake,
            server_ps,
            &ServerToClient::STCPServerStatus {
                protocol_version: PROTOCOL_VERSION,
                game_version: GAME_VERSION,
                supported_level_validators: vec!["lv".into()],
            },
        );
    }

    #[test]
    fn ready_gated_requests_refused_before_login() {
        let (mut client, _fake) = connected_client(7);
        assert!(!client.try_request_top_scores("lv"));
        assert!(!client.try_request_own_score("lv"));
        assert!(!client.try_request_top_scores_and_own_score("lv"));
        assert!(!client.try_send_started_game("lv"));
        let crf = oh_replay::CompressedReplayFile {
            data: vec![1, 2, 3],
        };
        assert!(!client.try_send_compressed_replay("lv", &crf));
    }

    #[test]
    fn compressed_replay_noop_when_level_unsupported() {
        let (mut client, fake) = connected_client(7);
        let server_ps = generate_sodium_ps_keys();
        to_ready(&mut client, &fake, &server_ps);
        let crf = oh_replay::CompressedReplayFile { data: vec![9, 9] };
        let before = fake.borrow().sent.len();
        // "lv3" is not in {lv1, lv2} → a `true` no-op
        // (`HexagonClient.cpp:893-899`).
        assert!(client.try_send_compressed_replay("lv3", &crf));
        assert_eq!(fake.borrow().sent.len(), before, "nothing was sent");
        // A supported level does send.
        assert!(client.try_send_compressed_replay("lv1", &crf));
        assert!(fake.borrow().sent.len() > before);
    }

    #[test]
    fn drop_disconnects_like_the_cpp_destructor() {
        let (client, fake) = connected_client(7);
        drop(client);
        assert!(fake.borrow().disconnects >= 1);
    }

    // -- framing (the SFML length-prefix state) ------------------------------

    /// A raw socket that reports scripted `Partial(3)`s then `Done`,
    /// recording every byte it "puts on the wire".
    struct ProgressSocket {
        calls: usize,
        sent: Vec<u8>,
    }

    impl NetSocket for ProgressSocket {
        fn connect(&mut self, _addr: SocketAddr, _timeout: Duration) -> RecvResult {
            RecvResult::Done(0)
        }
        fn send(&mut self, buf: &[u8]) -> SendResult {
            self.calls += 1;
            match self.calls {
                1 | 2 => {
                    // The `buf` here is the *remaining* range (the caller
                    // resumes from `m_sendPos`), so record the first 3.
                    self.sent.extend_from_slice(&buf[..3]);
                    SendResult::Partial(3)
                }
                _ => {
                    self.sent.extend_from_slice(buf);
                    SendResult::Done
                }
            }
        }
        fn recv(&mut self, _buf: &mut [u8]) -> RecvResult {
            RecvResult::NotReady
        }
        fn disconnect(&mut self) {}
    }

    #[test]
    fn framed_send_resumes_from_partial_progress() {
        let mut framed = FramedSocket::new(Box::new(ProgressSocket {
            calls: 0,
            sent: Vec::new(),
        }));
        let mut p = Packet::new();
        for b in [1u8, 2, 3, 4, 5, 6, 7, 8] {
            p.write_u8(b);
        }
        // The frame is 4 (size) + 8 (payload) = 12 bytes.
        assert_eq!(
            framed.send_packet(&p),
            SendResult::Partial(3),
            "first 3 of 12"
        );
        assert_eq!(
            framed.send_packet(&p),
            SendResult::Partial(3),
            "next 3 of the remainder"
        );
        assert_eq!(framed.send_packet(&p), SendResult::Done, "the rest");
    }

    #[test]
    fn framing_accumulates_a_frame_across_updates_and_raw_reads() {
        // A plain STCPKick frame (4 size bytes + 7 payload bytes) arrives
        // one raw-read chunk (≤ 3 bytes) at a time, across three
        // `update()` calls — exercising the C++ `m_pendingPacket`
        // size-then-payload accumulation (`TcpSocket.cpp:376-410`).
        let fake = Rc::new(RefCell::new(FakeSocket::new()));
        fake.borrow_mut().recv_chunk = 3;
        let mut client = HexagonClient::with_socket(
            Box::new(FakeSteam::immediate(7)),
            addr(),
            Box::new(SharedFake(fake.clone())),
            ClientClock::new(),
        );
        assert_eq!(client.poll_event(), Some(Event::ConnectionSuccess));

        let kick = frame(&plain_packet(&ServerToClient::STCPKick));
        assert_eq!(kick.len(), 11, "4 size bytes + 7 payload bytes");

        // update 1: only 2 of the 4 size bytes — nothing decodes.
        fake.borrow_mut().push_net(&kick[..2]);
        client.update();
        assert!(client.poll_event().is_none());
        assert_eq!(
            client.get_state(),
            State::Connected,
            "a partial frame changes nothing"
        );

        // update 2: the rest of the size + 2 payload bytes — still partial.
        fake.borrow_mut().push_net(&kick[2..8]);
        client.update();
        assert!(client.poll_event().is_none());

        // update 3: the final payload bytes — the frame completes.
        fake.borrow_mut().push_net(&kick[8..]);
        client.update();
        assert_eq!(client.poll_event(), Some(Event::Kicked));
        assert_eq!(client.get_state(), State::Disconnected);
    }

    // -- the heartbeat window --------------------------------------------------

    #[test]
    fn heartbeat_fires_only_after_45_seconds() {
        let start = Instant::now();
        let now_cell = Rc::new(std::cell::Cell::new(start));
        let fake = Rc::new(RefCell::new(FakeSocket::new()));
        let now_cell2 = now_cell.clone();
        let fake2 = fake.clone();
        let mut client = HexagonClient::with_socket(
            Box::new(FakeSteam::immediate(1)),
            addr(),
            Box::new(SharedFake(fake2)),
            ClientClock::custom(move || now_cell2.get(), |_| {} /* no real sleep */),
        );
        // connect() stamped last_heartbeat_time = start (via the fake now).
        let frames_before = fake.borrow().deframed().len();

        now_cell.set(start + Duration::from_secs(30));
        client.update();
        assert_eq!(
            fake.borrow().deframed().len(),
            frames_before,
            "no heartbeat at 30 s"
        );

        now_cell.set(start + Duration::from_secs(60));
        client.update();
        assert_eq!(
            fake.borrow().deframed().len(),
            frames_before + 1,
            "a heartbeat at 60 s (45 s window)"
        );
        assert_eq!(&fake.borrow().deframed().last().unwrap()[6..7], &[2]);
    }

    // -- the password hash ------------------------------------------------------

    #[test]
    fn salt_and_hash_pwd_prepends_the_salt() {
        // C++ `saltAndHashPwd` (`HexagonClient.cpp:799-811`):
        // `sodiumHash(salt + password)` with the `"salt"` fallback (the
        // `SecretPasswordSalt.hpp` header is absent from the tree).
        let got = salt_and_hash_pwd("password");
        let expected = crate::sodium::sodium_hash(b"saltpassword");
        assert_eq!(got.len(), 32, "a raw 32-byte digest, not UTF-8 text");
        assert_eq!(
            got.as_bytes(),
            &expected[..],
            "the wire string carries the raw digest bytes"
        );
        assert_ne!(
            got.as_bytes(),
            &crate::sodium::sodium_hash(b"password")[..],
            "the salt is used"
        );
        assert_ne!(
            got.as_bytes(),
            &crate::sodium::sodium_hash(b"passwordsalt")[..],
            "the salt is prepended, not appended"
        );
    }
}
