//! `HexagonServer` (issue 18) — the authoritative game server: the
//! `Core/HexagonServer.{hpp,cpp}` port (1430 lines).
//!
//! | C++ | Rust |
//! |---|---|
//! | `HexagonServer` | [`HexagonServer`] |
//! | `ConnectedClient` (private nested) | [`ConnectedClient`] |
//! | `ConnectedClient::State` | [`ClientState`] (the C++ numeric values) |
//! | `ConnectedClient::LoginData` | [`LoginData`] |
//! | `ConnectedClient::GameStatus` | [`GameStatus`] |
//! | `HexagonGame::GameExecutionResult` (the server's view) | [`GameExecutionResult`] |
//! | `HexagonGame& _hexagonGame` (the one method called: `runReplayUntilDeathAndGetScore`) | [`ReplayExecutor`] |
//! | `sf::SocketSelector` + `sf::TcpListener` + `sf::TcpSocket` + `sf::UdpSocket` | `std::net` non-blocking sockets + [`FrameReader`] (the fork's `m_pendingPacket` framed receive) |
//!
//! # Socket + threading model (the C++'s, verbatim)
//!
//! The C++ server is **single-threaded**: there is no accept thread and no
//! per-client thread. `HexagonServer` owns a blocking
//! `sf::TcpListener` (`.listen(port)`, `HexagonServer.cpp:98-110`), a UDP
//! control socket bound to `LocalHost` (`:82-96`), and one `sf::TcpSocket`
//! per client — all multiplexed by ONE `sf::SocketSelector`
//! (`initializeSocketSelector`, `:112-119`). `run()` (`:278-295`) loops
//! `runIteration()` (`:297-314`): `wait(30 s)` on the selector (the timeout
//! exists ONLY so the purges run even without traffic), then, if the
//! selector fired, `runIteration_Control` (`:316`),
//! `runIteration_TryAcceptingNewClient` (`:425` — one `accept` per
//! iteration, the new socket joins the selector), and
//! `runIteration_LoopOverSockets` (`:465` — for each READY client socket,
//! one framed `receive(Packet&)`, then `processPacket`; 5 consecutive
//! failures → kick + remove, `:485-503`). The per-iteration purges run
//! unconditionally: `runIteration_PurgeClients` (`:518` — `mustDisconnect`
//! or 60 s of inactivity, strict `>`), `runIteration_PurgeTokens`
//! (`:565` — hourly: kick logged-in clients whose tokens went stale +
//! `removeAllStaleLoginTokens`), `runIteration_FlushLogs` (`:604` — every
//! second).
//!
//! The port keeps the single-threaded model and swaps the blocking
//! select/poll for non-blocking `std::net` sockets: `run()`
//! (=[`HexagonServer::run`]) polls the listener + every client stream
//! each iteration, and sleeps 50 ms only when NOTHING was ready (the
//! C++'s 30 s `wait` timeout only fed the purges, which here run every
//! iteration, so the sleep interval is unobservable on the wire; the
//! 60 s / 5-failure / hourly / 1 s windows are unchanged).
//!
//! # The two seams (documented deviations, D-014)
//!
//! - `HexagonGame& _hexagonGame` → [`ReplayExecutor`]: the C++ calls
//!   exactly ONE method on the game (`runReplayUntilDeathAndGetScore`,
//!   `HexagonServer.cpp:683-687` — the replay simulation is issue 009's
//!   verified port, `oh_game::HexagonGame::run_replay_until_death_and_get_score`;
//!   the composition passes the real engine). The trait keeps `oh-net`
//!   free of the gameplay crate (the same seam style as the client's
//!   `SteamManager`, `client.rs`).
//! - `Database::` statics → the owned [`crate::database::Database`]
//!   instance (the C++ is a process singleton hardcoding `"ohdb.sqlite"`,
//!   `Database.cpp:44`; the composition supplies the opened instance,
//!   issue 17).
//!
//! The `HGAssets& _assets` reference becomes the composition's
//! `Arc<oh_assets::Assets>` (one owner, shared by reference — the C++
//! `mainServer` shares the same `HGAssets` between the game and the
//! server, `main.cpp:211-232`).
//!
//! Logging: the C++ `ssvu::lo("hg::HexagonServer::<fn>")` channel lines
//! become stdout `[hg::HexagonServer::<fn>]` lines (the port's logging
//! convention; the `SSVOH_SLOG_ERROR` prefix `[ERROR] ` is preserved).
//! The C++'s `clientAddr` pointer prints become a per-connection
//! monotonically increasing id (`'client #<id>'`). The C++'s
//! `_packetBuffer` member (the send/receive scratch `sf::Packet`) becomes
//! a local `Packet` per use (same content semantics).

use std::collections::HashSet;
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::database::Database;
use crate::db_records::{LoginToken, ProcessedScore, User};
use crate::packet::Packet;
use crate::protocol::{GAME_VERSION, PROTOCOL_VERSION};
use crate::shared::{
    decode_client_to_server_packet, make_server_to_client_encrypted_packet,
    make_server_to_client_packet, ClientToServer, ServerToClient,
};
use crate::sodium::{
    calculate_server_session_sodium_rt_keys, generate_sodium_ps_keys, random_uint64,
    sodium_key_to_string, SodiumPsKeys, SodiumPublicKeyArray, SodiumRtKeys,
};

/// C++ `ConnectedClient::State` (`HexagonServer.hpp:86-92`) — the C++
/// numeric values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientState {
    /// C++ `Disconnected = 0`.
    Disconnected = 0,
    /// C++ `Connected = 1`.
    Connected = 1,
    /// C++ `LoggedIn = 2`.
    LoggedIn = 2,
    /// C++ `LoggedIn_Ready = 3` (`LoggedInReady` — the crate convention,
    /// as `client.rs`'s `State::LoggedInReady`).
    LoggedInReady = 3,
}

impl ClientState {
    /// The C++ arms print `static_cast<int>(state)` — the discriminant.
    #[must_use]
    pub const fn as_int(self) -> i32 {
        self as i32
    }
}

/// C++ `ConnectedClient::LoginData` (`HexagonServer.hpp:103-110`).
#[derive(Debug, Clone)]
pub struct LoginData {
    /// C++ `_userId` (the `users` row id).
    pub user_id: u32,
    /// C++ `_steamId`.
    pub steam_id: u64,
    /// C++ `_name`.
    pub name: String,
    /// C++ `_passwordHash` (the raw wire bytes — a 32-byte `sodiumHash`
    /// digest, not UTF-8 in general).
    pub password_hash: String,
    /// C++ `_loginToken` (`randomUInt64()`).
    pub login_token: u64,
}

/// C++ `ConnectedClient::GameStatus` (`HexagonServer.hpp:114-118`).
#[derive(Debug, Clone)]
pub struct GameStatus {
    /// C++ `_startTP` (`Utils::SCClock::now()` = the SYSTEM clock,
    /// `Timestamp.hpp:14`).
    pub start_tp: SystemTime,
    /// C++ `_levelValidator`.
    pub level_validator: String,
}

/// The C++ `HexagonGame::GameExecutionResult` as the server reads it
/// (`HexagonGame.hpp` — `totalTimeSeconds` / `playedTimeSeconds` /
/// `pausedTimeSeconds` / `customScore`). `oh_game::GameExecutionResult` is
/// the engine-side port of the same C++ type; the composition's
/// [`ReplayExecutor`] adapts between them (one struct per crate — the
/// server only ever reads these four fields).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GameExecutionResult {
    /// C++ `totalTimeSeconds`.
    pub total_time_seconds: f64,
    /// C++ `playedTimeSeconds`.
    pub played_time_seconds: f64,
    /// C++ `pausedTimeSeconds`.
    pub paused_time_seconds: f64,
    /// C++ `customScore`.
    pub custom_score: f32,
}

/// The C++ `HexagonGame& _hexagonGame` seam — the ONE method the server
/// calls (`runReplayUntilDeathAndGetScore`, `HexagonServer.cpp:683-687`):
/// simulate the replay until the player dies (or `max_processing_seconds`
/// elapse), returning the execution result. `None` = the C++
/// `std::nullopt` (the max-processing-time / failure path).
pub trait ReplayExecutor: Send {
    /// C++ `_hexagonGame.runReplayUntilDeathAndGetScore(rf,
    /// maxProcessingSeconds, timescale)`.
    fn run_replay_until_death_and_get_score(
        &self,
        rf: &oh_replay::ReplayFile,
        max_processing_seconds: i32,
        timescale: f32,
    ) -> Option<GameExecutionResult>;
}

/// The outcome of one non-blocking framed read (the C++
/// `sf::TcpSocket::receive(Packet&)` `Status` for the ready-socket case,
/// fork `TcpSocket.cpp:368-424`).
enum Frame {
    /// C++ `Status::Done` — a complete frame's payload (a 0-length payload
    /// yields an empty `Vec` — the C++ returns `Done` with an empty
    /// packet, `:415-417`).
    Ready(Vec<u8>),
    /// Would block — the C++'s selector never marks such a socket ready,
    /// so the C++ never sees this (the port's non-blocking equivalent).
    WouldBlock,
    /// C++ `Status::Disconnected` (`recv() == 0`, `:297-300`).
    Disconnected,
    /// C++ `Status::Error`.
    Error,
}

/// The C++ `sf::TcpSocket`'s pending-receive state (`m_pendingPacket`,
/// fork `TcpSocket.cpp:376-410`): the 4 size bytes (u32-BE `htonl`), then
/// the payload, accumulated across reads. One per client (the C++'s is a
/// member of each `sf::TcpSocket`); reset with the client (the C++
/// `disconnect()` resets it, `:213-219`).
#[derive(Debug, Default)]
struct FrameReader {
    /// C++ `m_pendingPacket.Size`.
    size: [u8; 4],
    /// C++ `m_pendingPacket.SizeReceived`.
    size_pos: usize,
    /// C++ `m_pendingPacket.Data`.
    data: Vec<u8>,
}

impl FrameReader {
    /// C++ `TcpSocket::receive(Packet&)` (fork `TcpSocket.cpp:368-424`) in
    /// non-blocking form: the size phase (4 bytes), then the payload phase
    /// (1024-byte chunks until the declared size is reached); a complete
    /// frame returns `Ready` (the C++ `packet.onReceive` + `Done`), a
    /// partial one stays in `self` for the next call.
    fn try_read(&mut self, stream: &mut TcpStream) -> Frame {
        if self.size_pos < 4 {
            let chunk = &mut self.size[self.size_pos..];
            match stream.read(chunk) {
                Ok(0) => return Frame::Disconnected,
                Ok(n) => self.size_pos += n,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    return Frame::WouldBlock;
                }
                Err(_) => return Frame::Error,
            }
            // A partial size (the C++'s blocking `receive(Packet&)` would
            // have kept reading in one call — the port accumulates across
            // `try_read` calls instead): revisit next iteration.
            if self.size_pos < 4 {
                return Frame::WouldBlock;
            }
        }
        let packet_size = u32::from_be_bytes(self.size) as usize;
        if self.data.len() < packet_size {
            // C++: 1024-byte payload chunks (`:392-410`). The allocation is
            // bounded by the bytes ACTUALLY received (the C++ grows
            // `m_pendingPacket.Data` the same way — only `recv`-ed bytes
            // land in it).
            let size_to_get = (packet_size - self.data.len()).min(1024);
            let mut buf = vec![0u8; size_to_get];
            match stream.read(&mut buf) {
                Ok(0) => return Frame::Disconnected,
                Ok(n) => self.data.extend_from_slice(&buf[..n]),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    return Frame::WouldBlock;
                }
                Err(_) => return Frame::Error,
            }
            // A partial payload stays in `self` for the next call (the
            // C++'s `m_pendingPacket.Data` survives the same way).
            if self.data.len() < packet_size {
                return Frame::WouldBlock;
            }
        }
        if !self.data.is_empty() {
            // C++ `:413-422`: the pending state is reset after EVERY
            // complete packet (size + payload).
            self.size_pos = 0;
            return Frame::Ready(std::mem::take(&mut self.data));
        }
        // C++ `:415-417`: a 0-length payload still returns `Done` with an
        // empty packet.
        self.size_pos = 0;
        Frame::Ready(Vec::new())
    }
}

/// C++ `ConnectedClient` (`HexagonServer.hpp:78-124`).
#[derive(Debug)]
struct ConnectedClient {
    /// Per-connection id (the C++'s `static_cast<void*>(&c)` prints).
    client_id: u64,
    /// C++ `_socket` (an `sf::TcpSocket` — the port's non-blocking
    /// `TcpStream`).
    stream: TcpStream,
    /// The C++ `m_pendingPacket` framed-receive state (see [`FrameReader`]).
    frame: FrameReader,
    /// C++ `_lastActivity` (`Utils::SCTimePoint` = the system clock).
    last_activity: SystemTime,
    /// C++ `_consecutiveFailures`.
    consecutive_failures: i32,
    /// C++ `_mustDisconnect`.
    must_disconnect: bool,
    /// C++ `_clientPublicKey`.
    client_public_key: Option<SodiumPublicKeyArray>,
    /// C++ `_rtKeys`.
    rt_keys: Option<SodiumRtKeys>,
    /// C++ `_loginData`.
    login_data: Option<LoginData>,
    /// C++ `_state`.
    state: ClientState,
    /// C++ `_gameStatus`.
    /// C++ `_gameStatus` (`HexagonServer.hpp:114-118`) — set by
    /// `CTSPStartedGame`, read by `process_replay`.
    game_status: Option<GameStatus>,
}

impl ConnectedClient {
    fn new(client_id: u64, stream: TcpStream, last_activity: SystemTime) -> Self {
        Self {
            client_id,
            stream,
            frame: FrameReader::default(),
            last_activity,
            // C++ ctor: `_consecutiveFailures{0}`, `_mustDisconnect{false}`,
            // `_clientPublicKey{}`, `_loginData{}`, `_state{Disconnected}`.
            consecutive_failures: 0,
            must_disconnect: false,
            client_public_key: None,
            rt_keys: None,
            login_data: None,
            state: ClientState::Disconnected,
            game_status: None,
        }
    }
}

/// C++ `maxConsecutiveFailures` (`HexagonServer.cpp:489`).
const MAX_CONSECUTIVE_FAILURES: i32 = 5;
/// C++ `maxInactivity` (`HexagonServer.cpp:519`).
const MAX_INACTIVITY: Duration = Duration::from_secs(60);
/// The port's idle poll interval (see the module docs — the C++'s 30 s
/// selector timeout only fed the per-iteration purges, which run here
/// every iteration).
const IDLE_SLEEP: Duration = Duration::from_millis(50);
/// C++ local `topScoresLimit` (`HexagonServer.cpp:814`).
const TOP_SCORES_LIMIT: i32 = 6;

/// C++ `hg::HexagonServer` (`HexagonServer.hpp:50-200`).
pub struct HexagonServer {
    /// C++ `_assets` (the `HGAssets&` — the port shares the composition's
    /// owning `Arc`).
    assets: Arc<oh_assets::Assets>,
    /// C++ `_hexagonGame` — the [`ReplayExecutor`] seam.
    replay_executor: Box<dyn ReplayExecutor>,
    /// The C++ `Database::` process singleton — the composition's opened
    /// instance (issue 17: the path is the composition's, not the
    /// hardcoded `"ohdb.sqlite"`).
    db: Database,
    /// C++ `_supportedLevelValidators` (the `unordered_set` — membership).
    supported_level_validators: HashSet<String>,
    /// C++ `_supportedLevelValidatorsVector` (`Utils::toVector` of the set
    /// — the C++ order is unspecified; the port's is sorted, which the
    /// wire never pins).
    supported_level_validators_vector: Vec<String>,
    /// C++ `_serverIp` (the resolved IP).
    #[allow(dead_code)] // the C++'s `_serverIp` member (logged in the ctor)
    server_ip: IpAddr,
    /// C++ `_serverPort`.
    server_port: u16,
    /// C++ `_serverControlPort`.
    server_control_port: u16,
    /// C++ `_controlSocket` (a `sf::UdpSocket` bound to `LocalHost`).
    control_socket: UdpSocket,
    /// C++ `_listener` (an `sf::TcpListener` — the port's non-blocking
    /// `TcpListener`).
    listener: TcpListener,
    /// C++ `_running`.
    running: bool,
    /// C++ `_verbose`.
    verbose: bool,
    /// C++ `_connectedClients` (a `std::list` — the port's `Vec`; the
    /// list existed only for erase-while-iterating stability).
    clients: Vec<ConnectedClient>,
    /// The next client id (the C++'s per-`ConnectedClient` address).
    next_client_id: u64,
    /// C++ `_serverPSKeys` (`generateSodiumPSKeys()`).
    server_ps_keys: SodiumPsKeys,
    /// C++ `_errorOss` (a `std::ostringstream` — the port's `String`).
    error_oss: String,
    /// C++ `_lastTokenPurge`.
    last_token_purge: SystemTime,
    /// C++ `_lastLogsFlush`.
    last_logs_flush: SystemTime,
}

/// C++ `Utils::nowTimestamp()` (`Timestamp.cpp:15-22`) — epoch seconds.
#[must_use]
pub fn now_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// C++ `checkAndUpdateLastElapsed` (`HexagonServer.cpp:553-563`):
/// `if (now - last < duration) return false; last = now; true`.
fn check_and_update_last_elapsed(last: &mut SystemTime, duration: Duration) -> bool {
    let now = SystemTime::now();
    let Ok(elapsed) = now.duration_since(*last) else {
        return false;
    };
    if elapsed < duration {
        return false;
    }
    *last = now;
    true
}

/// C++ `makeSupportedLevelValidators`
/// (`HexagonServer.cpp:1298-1322`): for every non-unscored level, for
/// every difficulty mult, the level validator — kept only when the
/// whitelist contains it.
fn make_supported_level_validators(
    assets: &oh_assets::Assets,
    level_validator_whitelist: &HashSet<String>,
) -> Vec<String> {
    let mut result: HashSet<String> = HashSet::new();
    for level_data in assets.get_level_datas().values() {
        if level_data.unscored {
            continue;
        }
        for &dm in &level_data.difficulty_mults {
            // C++ `ld.getValidator(dm)` = the `validators[dm]` entry
            // (`getLevelValidator(packId + "_" + id, dm)` — `LevelData`'s
            // bit-keyed map).
            if let Some(validator) = level_data.validators.get(&dm.to_bits()) {
                if level_validator_whitelist.contains(validator) {
                    result.insert(validator.clone());
                }
            }
        }
    }
    let mut vector: Vec<String> = result.into_iter().collect();
    vector.sort();
    vector
}

impl HexagonServer {
    /// C++ `HexagonServer(...)` (`HexagonServer.cpp:1325-1411`), minus the
    /// SIGINT install (the port's composition installs the signal handler
    /// — `oh-net` has no signal dependency; the C++'s handler closes the
    /// listener + flips `_running`, `:1386-1398`) and minus the blocking
    /// `run()` call (the C++ runs the loop at the END of the ctor, `:1409`;
    /// the port's composition calls [`Self::run`] — same loop, same
    /// single-threaded model).
    ///
    /// `server_ip` is ALREADY resolved (the C++ receives the resolved
    /// `sf::IpAddress` — `main.cpp:229` `sf::IpAddress::resolve(...)`; the
    /// C++'s `TODO (P0): handle resolve errors` is the composition's).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        assets: Arc<oh_assets::Assets>,
        replay_executor: Box<dyn ReplayExecutor>,
        db: Database,
        server_ip: IpAddr,
        server_port: u16,
        server_control_port: u16,
        server_level_whitelist: HashSet<String>,
    ) -> Result<Self, std::io::Error> {
        let supported_level_validators_vector =
            make_supported_level_validators(&assets, &server_level_whitelist);
        let supported_level_validators: HashSet<String> =
            supported_level_validators_vector.iter().cloned().collect();

        // C++ `initializeControlSocket` (`:82-96`): the `bind` to
        // `LocalHost` on the control port. The port's non-blocking
        // equivalent: the ~zero read-timeout poll (std refuses a literal
        // 0-duration timeout; 1 ns behaves as non-blocking).
        let control_socket =
            UdpSocket::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), server_control_port))
                .map_err(|e| {
                    println!("[hg::HexagonServer::initializeControlSocket] [ERROR] Failure binding UDP control socket: {e}");
                    e
                })?;
        control_socket
            .set_read_timeout(Some(Duration::from_nanos(1)))
            .expect("UdpSocket read timeout");

        // C++ `initializeTcpListener` (`:98-110`): `listen(serverPort)`.
        let listener = TcpListener::bind(SocketAddr::new(server_ip, server_port)).map_err(|e| {
            println!("[hg::HexagonServer::initializeTcpListener] [ERROR] Failure initializing TCP listener: {e}");
            e
        })?;
        listener
            .set_nonblocking(true)
            .expect("TcpListener non-blocking");

        // C++ `initializeSocketSelector` (`:112-119`) — no port (the
        // non-blocking poll replaces the selector).

        let server_ps_keys = generate_sodium_ps_keys();
        let now = SystemTime::now();

        println!(
            "[hg::HexagonServer::HexagonServer] Initializing server...\n \
             - 'serverIp': '{server_ip}'\n \
             - 'serverPort': '{server_port}'\n \
             - 'serverControlPort': '{server_control_port}'\n \
             - 'sKeyPublic': '{}'\n \
             - 'sKeySecret': '{}'\n",
            sodium_key_to_string(&server_ps_keys.key_public),
            sodium_key_to_string(&server_ps_keys.key_secret)
        );

        let mut oss = String::from("Server initialized!\nSupported levels:\n");
        for level_validator in &supported_level_validators_vector {
            oss.push_str(&format!(" - {level_validator}\n"));
        }
        println!("[hg::HexagonServer::HexagonServer] {oss}\n");

        Ok(Self {
            assets,
            replay_executor,
            db,
            supported_level_validators,
            supported_level_validators_vector,
            server_ip,
            server_port,
            server_control_port,
            control_socket,
            listener,
            running: true,
            verbose: false,
            clients: Vec::new(),
            next_client_id: 1,
            server_ps_keys,
            error_oss: String::new(),
            last_token_purge: now,
            last_logs_flush: now,
        })
    }

    /// C++ `_running = false` (the SIGINT handler's half,
    /// `HexagonServer.cpp:1386-1398`).
    pub fn stop(&mut self) {
        self.running = false;
    }

    /// The bound TCP port (port 0 → the OS-assigned one; tests + the
    /// composition probe it). No C++ equivalent (the C++'s port is always
    /// the configured one).
    #[must_use]
    pub fn port(&self) -> u16 {
        self.listener
            .local_addr()
            .map(|a| a.port())
            .unwrap_or(self.server_port)
    }

    /// The bound control (UDP) port (the `port()` twin for the control
    /// socket — the C++'s control port is always the configured one).
    #[must_use]
    pub fn control_port(&self) -> u16 {
        self.control_socket
            .local_addr()
            .map(|a| a.port())
            .unwrap_or(self.server_control_port)
    }

    /// The composition's DB (the C++'s `Database::` singleton).
    #[must_use]
    pub fn db(&self) -> &Database {
        &self.db
    }

    /// The composition's DB, mutably (the write-side `add_user`/
    /// `add_score`/`add_login_token` surface — the C++'s DB statics are
    /// `void`-returning, so the port's `Result`-returning writes need the
    /// `&mut`).
    #[must_use]
    pub fn db_mut(&mut self) -> &mut Database {
        &mut self.db
    }

    /// The C++'s `_supportedLevelValidatorsVector` (what
    /// `STCPServerStatus` sends).
    #[must_use]
    pub fn supported_level_validators(&self) -> &[String] {
        &self.supported_level_validators_vector
    }

    /// C++ `_connectedClients.size()` (diagnostics).
    #[must_use]
    pub fn client_count(&self) -> usize {
        self.clients.len()
    }

    /// C++ `isLevelSupported` (`HexagonServer.cpp:76-79`).
    fn is_level_supported(&self, level_validator: &str) -> bool {
        self.supported_level_validators.contains(level_validator)
    }

    /// C++ `slog` / `SSVOH_SLOG` (`HexagonServer.cpp:47-52`).
    fn slog(func: &str, msg: &str) {
        println!("[hg::HexagonServer::{func}] {msg}");
    }

    /// C++ `SSVOH_SLOG_VERBOSE` (`:54-55`).
    fn slog_verbose(&self, func: &str, msg: &str) {
        if self.verbose {
            Self::slog(func, msg);
        }
    }

    /// C++ `SSVOH_SLOG_ERROR` (`:57`).
    fn slog_error(func: &str, msg: &str) {
        println!("[hg::HexagonServer::{func}] [ERROR] {msg}");
    }

    /// C++ `fail<Ts...>` (`HexagonServer.cpp:64-74`): the error log line +
    /// `false`.
    fn fail(&self, func: &str, msg: &str) -> bool {
        Self::slog_error(func, msg);
        false
    }

    /// The C++ `sf::TcpSocket::send(Packet&)` frame over one client stream
    /// (u32-BE size + payload, fork `TcpSocket.cpp:309-365`) — a free
    /// helper so the index-based senders keep ONE borrow of the client at
    /// a time (the C++'s `sendPacket` member, `HexagonServer.cpp:120-129`).
    fn send_frame(c: &mut ConnectedClient, payload: &[u8]) -> bool {
        let mut frame = Vec::with_capacity(4 + payload.len());
        frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        frame.extend_from_slice(payload);
        // The C++ socket is blocking; the port's is non-blocking — a
        // `WouldBlock`/error is the C++'s `Status != Done`.
        c.stream.write_all(&frame).is_ok()
    }

    /// C++ `sendPacket` (`HexagonServer.cpp:120-129`), index-based.
    fn send_packet_to(&mut self, i: usize, p: &Packet) -> bool {
        let payload = p.data();
        let sent = self
            .clients
            .get_mut(i)
            .is_some_and(|c| Self::send_frame(c, payload));
        if !sent {
            self.fail("sendPacket", "Failure sending packet");
        }
        sent
    }

    /// C++ `sendKick` (`HexagonServer.cpp:153-157`).
    fn send_kick(&mut self, i: usize) -> bool {
        let mut p = Packet::new();
        make_server_to_client_packet(&mut p, &ServerToClient::STCPKick);
        self.send_packet_to(i, &p)
    }

    /// C++ `sendRegistrationSuccess` (`HexagonServer.cpp:167-170`).
    fn send_registration_success(&mut self, i: usize) -> bool {
        self.send_encrypted(i, &ServerToClient::STCPRegistrationSuccess)
    }

    /// C++ `sendRegistrationFailure` (`HexagonServer.cpp:172-176`).
    fn send_registration_failure(&mut self, i: usize, error: &str) -> bool {
        self.send_encrypted(
            i,
            &ServerToClient::STCPRegistrationFailure {
                error: error.to_string(),
            },
        )
    }

    /// C++ `sendLoginSuccess` (`HexagonServer.cpp:178-187`).
    fn send_login_success(&mut self, i: usize, login_token: u64, login_name: &str) -> bool {
        self.send_encrypted(
            i,
            &ServerToClient::STCPLoginSuccess {
                login_token,
                login_name: login_name.to_string(),
            },
        )
    }

    /// C++ `sendLoginFailure` (`HexagonServer.cpp:189-193`).
    fn send_login_failure(&mut self, i: usize, error: &str) -> bool {
        self.send_encrypted(
            i,
            &ServerToClient::STCPLoginFailure {
                error: error.to_string(),
            },
        )
    }

    /// C++ `sendLogoutSuccess` (`HexagonServer.cpp:195-198`).
    fn send_logout_success(&mut self, i: usize) -> bool {
        self.send_encrypted(i, &ServerToClient::STCPLogoutSuccess)
    }

    /// C++ `sendLogoutFailure` (`HexagonServer.cpp:200-203`).
    fn send_logout_failure(&mut self, i: usize) -> bool {
        self.send_encrypted(i, &ServerToClient::STCPLogoutFailure)
    }

    /// C++ `sendDeleteAccountSuccess` (`HexagonServer.cpp:205-208`).
    fn send_delete_account_success(&mut self, i: usize) -> bool {
        self.send_encrypted(i, &ServerToClient::STCPDeleteAccountSuccess)
    }

    /// C++ `sendDeleteAccountFailure` (`HexagonServer.cpp:210-214`).
    fn send_delete_account_failure(&mut self, i: usize, error: &str) -> bool {
        self.send_encrypted(
            i,
            &ServerToClient::STCPDeleteAccountFailure {
                error: error.to_string(),
            },
        )
    }

    /// C++ `sendTopScores` (`HexagonServer.cpp:216-226`).
    fn send_top_scores(
        &mut self,
        i: usize,
        level_validator: &str,
        scores: &[ProcessedScore],
    ) -> bool {
        self.send_encrypted(
            i,
            &ServerToClient::STCPTopScores {
                level_validator: level_validator.to_string(),
                scores: scores.to_vec(),
            },
        )
    }

    /// C++ `sendOwnScore` (`HexagonServer.cpp:228-237`).
    fn send_own_score(&mut self, i: usize, level_validator: &str, score: &ProcessedScore) -> bool {
        self.send_encrypted(
            i,
            &ServerToClient::STCPOwnScore {
                level_validator: level_validator.to_string(),
                score: score.clone(),
            },
        )
    }

    /// C++ `sendTopScoresAndOwnScore` (`HexagonServer.cpp:239-251`).
    fn send_top_scores_and_own_score(
        &mut self,
        i: usize,
        level_validator: &str,
        scores: &[ProcessedScore],
        own_score: &Option<ProcessedScore>,
    ) -> bool {
        self.send_encrypted(
            i,
            &ServerToClient::STCPTopScoresAndOwnScore {
                level_validator: level_validator.to_string(),
                scores: scores.to_vec(),
                own_score: own_score.clone(),
            },
        )
    }

    /// C++ `sendServerStatus` (`HexagonServer.cpp:253-263`): the C++
    /// passes `PROTOCOL_VERSION`/`GAME_VERSION` + the
    /// `_supportedLevelValidatorsVector`; the port the same (the
    /// `crate::protocol` constants + the constructor-computed vector).
    fn send_server_status(&mut self, i: usize) -> bool {
        self.send_encrypted(
            i,
            &ServerToClient::STCPServerStatus {
                protocol_version: PROTOCOL_VERSION,
                game_version: GAME_VERSION,
                supported_level_validators: self.supported_level_validators_vector.clone(),
            },
        )
    }

    /// C++ `sendEncrypted<T>` (`HexagonServer.cpp:131-151`): the RT-key
    /// check + `makeServerToClientEncryptedPacket` (the C++'s build-failure
    /// branch is unreachable — the Rust `make_*` never fails, `shared.rs`
    /// docs) + the framed send.
    fn send_encrypted(&mut self, i: usize, data: &ServerToClient) -> bool {
        let key_transmit = match self.clients[i].rt_keys.as_ref() {
            Some(rt) => rt.key_transmit,
            None => {
                return self.fail(
                    "sendEncrypted",
                    &format!(
                        "Tried to send encrypted message without RT keys for client '{}'",
                        self.clients[i].client_id
                    ),
                );
            }
        };
        let mut p = Packet::new();
        make_server_to_client_encrypted_packet(&key_transmit, &mut p, data);
        self.send_packet_to(i, &p)
    }

    /// C++ `sendPublicKey` (`HexagonServer.cpp:159-165`).
    fn send_public_key(&mut self, i: usize) -> bool {
        let mut p = Packet::new();
        make_server_to_client_packet(
            &mut p,
            &ServerToClient::STCPPublicKey {
                key: self.server_ps_keys.key_public,
            },
        );
        self.send_packet_to(i, &p)
    }

    /// C++ `kickAndRemoveClient` (`HexagonServer.cpp:266-276`): the kick,
    /// the logged-in user's token removal, and the selector removal (no
    /// port — the non-blocking model has no selector; the client leaves
    /// the list at the call site).
    fn kick_and_remove_client(&mut self, i: usize) {
        self.send_kick(i);

        if let Some(user_id) = self.clients[i].login_data.as_ref().map(|ld| ld.user_id) {
            if let Err(e) = self.db.remove_all_login_tokens_for_user(user_id) {
                Self::slog_error(
                    "kickAndRemoveClient",
                    &format!("Failure removing login tokens for user '{user_id}': {e}"),
                );
            }
        }
    }

    // ------------------------------------------------------------------
    // The iteration (C++ `run` / `runIteration`)
    // ------------------------------------------------------------------

    /// C++ `run` (`HexagonServer.cpp:278-295`): the loop. The C++'s
    /// `try/catch` has no port (the methods return `bool`, not throw).
    /// `stop` is the composition's signal-hook flag (the C++'s SIGINT
    /// handler — see the module docs).
    pub fn run(&mut self, stop: &AtomicBool) {
        while self.running && !stop.load(Ordering::SeqCst) {
            let activity = self.run_iteration();
            if !activity {
                // The C++'s `wait(30 s)` blocks here on the selector; the
                // port's non-blocking poll sleeps (the module docs).
                std::thread::sleep(IDLE_SLEEP);
            }
        }
    }

    /// C++ `runIteration` (`HexagonServer.cpp:297-314`): one pass over the
    /// control socket, the listener, and every client (the C++ gates those
    /// three on the selector firing — the port polls them directly), then
    /// the three unconditional purges. Returns whether ANY I/O happened
    /// (the C++'s `wait` returning true for traffic, as opposed to its
    /// 30 s timeout).
    pub fn run_iteration(&mut self) -> bool {
        self.slog_verbose("runIteration", "New iteration...\n");

        let mut activity = false;
        activity |= self.run_iteration_control();
        activity |= self.run_iteration_try_accepting_new_client();

        let mut removed: Vec<u64> = Vec::new();
        activity |= self.run_iteration_loop_over_sockets(&mut removed);

        self.run_iteration_purge_clients(&removed);
        self.run_iteration_purge_tokens();
        self.run_iteration_flush_logs();

        activity
    }

    /// C++ `runIteration_Control` (`HexagonServer.cpp:316-423`): read one
    /// control datagram (the C++'s ready-gated blocking `receive` — the
    /// port's zero-timeout poll), decode the length-prefixed string, and
    /// the `verbose true|false` command (`db exec` is `#if 0` in the C++
    /// — not shipped).
    fn run_iteration_control(&mut self) -> bool {
        let mut buf = [0u8; 4096];
        let (n, _sender) = match self.control_socket.recv_from(&mut buf) {
            Ok(x) => x,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => return false,
            Err(e) => {
                return self.fail(
                    "runIteration_Control",
                    &format!("Failure receiving control packet: {e}"),
                );
            }
        };

        let mut p = Packet::new();
        p.append(&buf[..n]);
        let control_msg = match p.read_str() {
            Some(s) => s,
            None => {
                return self.fail("runIteration_Control", "Failure decoding control packet");
            }
        };

        Self::slog(
            "runIteration_Control",
            &format!("Received control packet, contents: '{control_msg}'\n"),
        );

        if control_msg.is_empty() {
            return true;
        }

        // C++ `Utils::split<std::string>(controlMsg)` (`Utils/Split.hpp`):
        // space-delimited, EMPTY pieces skipped.
        let splitted: Vec<&str> = control_msg.split(' ').filter(|s| !s.is_empty()).collect();
        if splitted.is_empty() {
            return true;
        }

        if splitted[0] == "verbose" {
            if splitted.len() != 2 {
                Self::slog_error(
                    "runIteration_Control",
                    "'verbose' command must be followed by 'true' or 'false'\n",
                );
                return true;
            }
            if splitted[1] == "true" {
                Self::slog("runIteration_Control", "Enabled verbose mode\n");
                self.verbose = true;
                return true;
            }
            if splitted[1] == "false" {
                Self::slog("runIteration_Control", "Disabled verbose mode\n");
                self.verbose = false;
                return true;
            }
        }

        true
    }

    /// C++ `runIteration_TryAcceptingNewClient`
    /// (`HexagonServer.cpp:425-463`): the ready listener → `accept` (the
    /// port drains ALL pending connections — the C++ accepts one per
    /// iteration and the selector re-fires; same net effect). The new
    /// client starts in `State::Connected` (the C++ sets it after the
    /// `accept`, `:454`).
    fn run_iteration_try_accepting_new_client(&mut self) -> bool {
        let mut accepted = false;
        loop {
            match self.listener.accept() {
                Ok((stream, _addr)) => {
                    if stream.set_nonblocking(true).is_err() {
                        Self::slog(
                            "runIteration_TryAcceptingNewClient",
                            "Listener failed to accept new client (non-blocking)\n",
                        );
                        continue;
                    }
                    let client_id = self.next_client_id;
                    self.next_client_id += 1;
                    let now = SystemTime::now();
                    let mut client = ConnectedClient::new(client_id, stream, now);
                    client.state = ClientState::Connected;
                    Self::slog(
                        "runIteration_TryAcceptingNewClient",
                        &format!("Listener accepted new client '#{client_id}'\n"),
                    );
                    self.clients.push(client);
                    accepted = true;
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => {
                    Self::slog(
                        "runIteration_TryAcceptingNewClient",
                        &format!("Listener failed to accept new client ('{e}')\n"),
                    );
                    break;
                }
            }
        }
        accepted
    }

    /// C++ `runIteration_LoopOverSockets`
    /// (`HexagonServer.cpp:465-516`): for each client with data ready, one
    /// framed `receive` + `processPacket`; success refreshes
    /// `_lastActivity` + resets `_consecutiveFailures`, failure increments
    /// it, and the 5th consecutive failure kicks + removes.
    fn run_iteration_loop_over_sockets(&mut self, removed: &mut Vec<u64>) -> bool {
        let mut activity = false;
        let mut i = 0;
        while i < self.clients.len() {
            let frame = {
                let c = &mut self.clients[i];
                c.frame.try_read(&mut c.stream)
            };
            match frame {
                Frame::Ready(payload) => {
                    activity = true;
                    // C++ `_packetBuffer.clear()` + the receive into it.
                    let mut p = Packet::new();
                    p.append(&payload);
                    let processed = self.process_packet(i, &mut p);
                    if processed {
                        self.clients[i].last_activity = SystemTime::now();
                        self.clients[i].consecutive_failures = 0;
                    } else if self.handle_client_failure(i, removed) {
                        continue; // the client was removed — re-index `i`
                    }
                }
                Frame::WouldBlock => {
                    // The C++'s selector marks such a socket not-ready;
                    // nothing to do (the client is revisited next
                    // iteration).
                }
                Frame::Disconnected | Frame::Error => {
                    if self.handle_client_failure(i, removed) {
                        continue;
                    }
                }
            }
            i += 1;
        }
        activity
    }

    /// The C++ failure tail of `runIteration_LoopOverSockets`
    /// (`HexagonServer.cpp:485-503`): the failure log, the counter, and
    /// the 5th-failure kick + erase. Returns `true` when the client was
    /// removed (the caller must re-index `i`).
    fn handle_client_failure(&mut self, i: usize, removed: &mut Vec<u64>) -> bool {
        let client_id = self.clients[i].client_id;
        self.slog_verbose(
            "runIteration_LoopOverSockets",
            &format!(
                "Failed to receive data from client '#{client_id}' (consecutive failures: {})\n",
                self.clients[i].consecutive_failures
            ),
        );

        self.clients[i].consecutive_failures += 1;

        if self.clients[i].consecutive_failures == MAX_CONSECUTIVE_FAILURES {
            Self::slog(
                "runIteration_LoopOverSockets",
                &format!(
                    "Too many consecutive failures for client '#{client_id}', removing from list\n"
                ),
            );
            self.kick_and_remove_client(i);
            self.clients.remove(i);
            removed.push(client_id);
            return true;
        }
        false
    }

    /// C++ `runIteration_PurgeClients`
    /// (`HexagonServer.cpp:518-547`): `mustDisconnect` or
    /// `now - lastActivity > 60 s` (strict `>`) → kick + remove.
    fn run_iteration_purge_clients(&mut self, removed: &[u64]) {
        let now = SystemTime::now();
        let to_remove: Vec<usize> = self
            .clients
            .iter()
            .enumerate()
            .filter(|(_, c)| {
                !removed.contains(&c.client_id)
                    && (c.must_disconnect
                        || matches!(
                            now.duration_since(c.last_activity),
                            Ok(elapsed) if elapsed > MAX_INACTIVITY
                        ))
            })
            .map(|(i, _)| i)
            .collect();

        for &i in to_remove.iter().rev() {
            let must_disconnect = self.clients[i].must_disconnect;
            let client_id = self.clients[i].client_id;
            if must_disconnect {
                Self::slog(
                    "runIteration_PurgeClients",
                    &format!("Client '#{client_id}' disconnected, removing from list\n"),
                );
            } else {
                Self::slog(
                    "runIteration_PurgeClients",
                    &format!("Client '#{client_id}' timed out, removing from list\n"),
                );
            }
            self.kick_and_remove_client(i);
            self.clients.remove(i);
        }
    }

    /// C++ `runIteration_PurgeTokens`
    /// (`HexagonServer.cpp:565-602`): hourly — kick the logged-in clients
    /// whose tokens went stale, then `removeAllStaleLoginTokens`.
    fn run_iteration_purge_tokens(&mut self) {
        if !check_and_update_last_elapsed(&mut self.last_token_purge, Duration::from_secs(3600)) {
            return;
        }

        self.slog_verbose("runIteration_PurgeTokens", "Purging old login tokens\n");

        let stale = match self.db.get_all_stale_login_tokens() {
            Ok(tokens) => tokens,
            Err(e) => {
                self.fail(
                    "runIteration_PurgeTokens",
                    &format!("Failure getting stale login tokens: {e}"),
                );
                return;
            }
        };

        for lt in &stale {
            Self::slog(
                "runIteration_PurgeTokens",
                &format!("Found stale token for user '{}'\n", lt.user_id),
            );
            let to_remove: Vec<usize> = self
                .clients
                .iter()
                .enumerate()
                .filter(|(_, c)| {
                    c.login_data
                        .as_ref()
                        .is_some_and(|ld| ld.user_id == lt.user_id)
                })
                .map(|(i, _)| i)
                .collect();
            for &i in to_remove.iter().rev() {
                Self::slog(
                    "runIteration_PurgeTokens",
                    &format!(
                        "Kicking stale token client '{}'\n",
                        self.clients[i].client_id
                    ),
                );
                self.kick_and_remove_client(i);
                self.clients.remove(i);
            }
        }

        if let Err(e) = self.db.remove_all_stale_login_tokens() {
            self.fail(
                "runIteration_PurgeTokens",
                &format!("Failure removing stale login tokens: {e}"),
            );
        }
    }

    /// C++ `runIteration_FlushLogs` (`HexagonServer.cpp:604-614`): every
    /// second, flush stdout (+ stderr + the `ssvu::lo` sink — the port has
    /// one sink: stdout).
    fn run_iteration_flush_logs(&mut self) {
        if !check_and_update_last_elapsed(&mut self.last_logs_flush, Duration::from_secs(1)) {
            return;
        }
        let _ = std::io::stdout().flush();
    }

    /// C++ `validateLogin` (`HexagonServer.cpp:616-640`).
    fn validate_login(&self, i: usize, context: &str, ctsp_login_token: u64) -> bool {
        if self.clients[i].login_data.is_none() {
            Self::slog(
                "validateLogin",
                &format!(
                    "Client '{}' , is not logged in for {context}\n",
                    self.clients[i].client_id
                ),
            );
            return false;
        }

        let c_login_token = self.clients[i]
            .login_data
            .as_ref()
            .expect("checked above")
            .login_token;
        if c_login_token != ctsp_login_token {
            Self::slog(
                "validateLogin",
                &format!(
                    "Client '{}' login token mismatch for {context}\n",
                    self.clients[i].client_id
                ),
            );
            return false;
        }

        true
    }

    // ------------------------------------------------------------------
    // The packet dispatch (C++ `processPacket`)
    // ------------------------------------------------------------------

    /// C++ `checkState` lambda (`HexagonServer.cpp:821-831`) — the C++'s
    /// exact verbose line (incl. its trailing double-quote typo).
    fn check_state(&self, i: usize, state: ClientState) -> bool {
        if self.clients[i].state != state {
            self.slog_verbose(
                "processPacket",
                &format!(
                    "Invalid client state, expected '{}', state was '{}''\n",
                    state.as_int(),
                    self.clients[i].state.as_int()
                ),
            );
            return false;
        }
        true
    }

    /// C++ `printCTSPDataVerbose` (`HexagonServer.cpp:749-807`): the
    /// per-field dump (declaration order — the C++'s `boost::pfr` iteration)
    /// when verbose. The port prints the same fields in the same order:
    /// `sodiumKeyToString` for the key, the `<REPLAY_FILE>` /
    /// `<COMPRESSED_REPLAY_FILE>` markers, the strings as-is, and
    /// `std::to_string` (decimal) for the scalars.
    fn print_ctsp_data_verbose(&self, i: usize, title: &str, ctsp: &ClientToServer) {
        if !self.verbose {
            return;
        }
        let fields: Vec<String> = match ctsp {
            ClientToServer::CTSPPublicKey { key } => vec![sodium_key_to_string(key)],
            ClientToServer::CTSPRegister {
                steam_id,
                name,
                password_hash,
            }
            | ClientToServer::CTSPLogin {
                steam_id,
                name,
                password_hash,
            } => {
                vec![steam_id.to_string(), name.clone(), password_hash.clone()]
            }
            ClientToServer::CTSPLogout { steam_id } => vec![steam_id.to_string()],
            ClientToServer::CTSPDeleteAccount {
                steam_id,
                password_hash,
            } => {
                vec![steam_id.to_string(), password_hash.clone()]
            }
            ClientToServer::CTSPRequestTopScores {
                login_token,
                level_validator,
            }
            | ClientToServer::CTSPRequestOwnScore {
                login_token,
                level_validator,
            }
            | ClientToServer::CTSPRequestTopScoresAndOwnScore {
                login_token,
                level_validator,
            }
            | ClientToServer::CTSPStartedGame {
                login_token,
                level_validator,
            } => {
                vec![login_token.to_string(), level_validator.clone()]
            }
            ClientToServer::CTSPReplay {
                login_token,
                replay_file: _,
            } => vec![login_token.to_string(), "<REPLAY_FILE>".to_string()],
            ClientToServer::CTSPCompressedReplay {
                login_token,
                compressed_replay_file: _,
            } => vec![
                login_token.to_string(),
                "<COMPRESSED_REPLAY_FILE>".to_string(),
            ],
            ClientToServer::CTSPRequestServerStatus { login_token }
            | ClientToServer::CTSPReady { login_token } => vec![login_token.to_string()],
            ClientToServer::CTSPHeartbeat | ClientToServer::CTSPDisconnect => Vec::new(),
            ClientToServer::PInvalid { .. } | ClientToServer::PEncryptedMsg { .. } => Vec::new(),
        };
        Self::slog(
            "printCTSPDataVerbose",
            &format!(
                "Received '{title}' packet from client '{}', contents: {{{}}}\n",
                self.clients[i].client_id,
                fields.join(", ")
            ),
        );
    }

    /// C++ arm-local `sendFail` lambda (e.g. `HexagonServer.cpp:945-950`):
    /// the error log line, then the encrypted failure send.
    fn log_send_fail(&self, error: &str) {
        Self::slog("processPacket", &format!("{error}\n"));
    }

    /// The C++ DB statics never report errors (they just execute); the
    /// port's `Result`-returning `Database` does — on `Err`, the C++
    /// equivalent of "the DB call blew up" is the `fail` path.
    fn db_fail(&self, error: &rusqlite::Error) -> bool {
        self.fail("processPacket", &format!("Database error: {error}"))
    }

    /// C++ `CTSPPublicKey` arm (`HexagonServer.cpp:893-934`): store the
    /// client key, calculate the RT keys (failure → kick + disconnect),
    /// reply with the server's public key.
    fn handle_public_key(&mut self, i: usize, key: SodiumPublicKeyArray) -> bool {
        self.print_ctsp_data_verbose(i, "public key", &ClientToServer::CTSPPublicKey { key });

        if self.clients[i].client_public_key.is_some() {
            self.slog_verbose("processPacket", "Already had public key, replacing\n");
        } else {
            self.slog_verbose("processPacket", "Did not have public key, setting\n");
        }
        self.clients[i].client_public_key = Some(key);

        self.slog_verbose(
            "processPacket",
            &format!("Client public key: '{}'\n", sodium_key_to_string(&key)),
        );
        self.slog_verbose("processPacket", "Calculating RT keys\n");

        match calculate_server_session_sodium_rt_keys(&self.server_ps_keys, &key) {
            Some(rt_keys) => {
                self.clients[i].rt_keys = Some(rt_keys);
                self.slog_verbose(
                    "processPacket",
                    &format!(
                        "Calculated RT keys\n - 'keyReceive': '{}'\n - 'keyTransmit': '{}'\n",
                        sodium_key_to_string(&rt_keys.key_receive),
                        sodium_key_to_string(&rt_keys.key_transmit)
                    ),
                );
                Self::slog("processPacket", "Replying with own public key\n");
                self.send_public_key(i)
            }
            None => {
                Self::slog_error(
                    "processPacket",
                    &format!(
                        "Failed calculating RT keys, disconnecting client '{}'\n",
                        self.clients[i].client_id
                    ),
                );
                self.clients[i].must_disconnect = true;
                self.send_kick(i);
                false
            }
        }
    }

    /// C++ `CTSPRegister` arm (`HexagonServer.cpp:936-983`): the
    /// connected-state gate, the 32-byte name gate, the steamId/name
    /// uniqueness checks, the `addUser` (the `passwordHash` = the raw wire
    /// bytes, the C++'s `Utils::stringToCharVec`), and the encrypted
    /// success/failure reply. The server NEVER re-hashes: the C++ client
    /// sends the `sodiumHash(salt + password)` digest and the server
    /// stores/compares the raw bytes (the milestone-2 tests pin the exact
    /// digest the C++ `saltAndHashPwd` produces).
    fn handle_register(
        &mut self,
        i: usize,
        steam_id: u64,
        name: &str,
        password_hash: &str,
    ) -> bool {
        self.print_ctsp_data_verbose(
            i,
            "register",
            &ClientToServer::CTSPRegister {
                steam_id,
                name: name.to_string(),
                password_hash: password_hash.to_string(),
            },
        );

        if !self.check_state(i, ClientState::Connected) {
            self.log_send_fail("Client not in connected state");
            return self.send_registration_failure(i, "Client not in connected state");
        }

        // C++ `name.size()` = the BYTE count (`std::string::size`).
        if name.len() > 32 {
            self.log_send_fail("Name too long, max is 32 characters");
            return self.send_registration_failure(i, "Name too long, max is 32 characters");
        }

        if let Ok(any) = self.db.any_user_with_steam_id(steam_id) {
            if any {
                let error = format!("User with steamId '{steam_id}' already registered");
                self.log_send_fail(&error);
                return self.send_registration_failure(i, &error);
            }
        } else if let Err(e) = self.db.any_user_with_steam_id(steam_id) {
            return self.db_fail(&e);
        }

        if let Ok(any) = self.db.any_user_with_name(name) {
            if any {
                let error = format!("User with name '{name}' already registered");
                self.log_send_fail(&error);
                return self.send_registration_failure(i, &error);
            }
        } else if let Err(e) = self.db.any_user_with_name(name) {
            return self.db_fail(&e);
        }

        // C++ `Database::addUser(Database::User{.steamId, .name,
        // .passwordHash = Utils::stringToCharVec(passwordHash)})` — the
        // raw wire bytes into the BLOB column.
        let user = User {
            id: 0,
            steam_id,
            name: name.to_string(),
            password_hash: password_hash.as_bytes().to_vec(),
        };
        if let Err(e) = self.db.add_user(&user) {
            return self.db_fail(&e);
        }

        Self::slog("processPacket", "Successfully registered\n");
        self.send_registration_success(i)
    }

    /// C++ `CTSPLogin` arm (`HexagonServer.cpp:985-1056`): the connected
    /// gate, the name/steamId existence checks (the C++'s two `anyUser*`
    /// lines), the `getUserWithSteamIdAndName` + raw-byte password
    /// comparison, the token issuance (`removeAllLoginTokensForUser` +
    /// `addLoginToken` with `randomUInt64()` + `nowTimestamp()`), the
    /// session login data, and the encrypted `STCPLoginSuccess`.
    fn handle_login(&mut self, i: usize, steam_id: u64, name: &str, password_hash: &str) -> bool {
        self.print_ctsp_data_verbose(
            i,
            "login",
            &ClientToServer::CTSPLogin {
                steam_id,
                name: name.to_string(),
                password_hash: password_hash.to_string(),
            },
        );

        if !self.check_state(i, ClientState::Connected) {
            self.log_send_fail("Client not in connected state");
            return self.send_login_failure(i, "Client not in connected state");
        }

        if name.len() > 32 {
            self.log_send_fail("Name too long, max is 32 characters");
            return self.send_login_failure(i, "Name too long, max is 32 characters");
        }

        if let Ok(any) = self.db.any_user_with_steam_id(steam_id) {
            if !any {
                let error = format!("No user with steamId '{steam_id}' registered");
                self.log_send_fail(&error);
                return self.send_login_failure(i, &error);
            }
        } else if let Err(e) = self.db.any_user_with_steam_id(steam_id) {
            return self.db_fail(&e);
        }
        if let Ok(any) = self.db.any_user_with_name(name) {
            if !any {
                let error = format!("No user with name '{name}' registered");
                self.log_send_fail(&error);
                return self.send_login_failure(i, &error);
            }
        } else if let Err(e) = self.db.any_user_with_name(name) {
            return self.db_fail(&e);
        }

        let user = match self.db.get_user_with_steam_id_and_name(steam_id, name) {
            Ok(user) => user,
            Err(e) => return self.db_fail(&e),
        };
        let user = match user {
            Some(user) => user,
            None => {
                let error = format!("No user matching '{steam_id}' and '{name}' registered");
                self.log_send_fail(&error);
                return self.send_login_failure(i, &error);
            }
        };

        // C++ `user->passwordHash != Utils::stringToCharVec(passwordHash)`
        // — the raw 32-byte digest comparison (no hashing on the server).
        if user.password_hash != password_hash.as_bytes() {
            let error = format!("Invalid password for user matching '{steam_id}' and '{name}'");
            self.log_send_fail(&error);
            return self.send_login_failure(i, &error);
        }

        Self::slog("processPacket", "Creating login token for user\n");

        let login_token = random_uint64();
        if let Err(e) = self.db.remove_all_login_tokens_for_user(user.id) {
            return self.db_fail(&e);
        }
        if let Err(e) = self.db.add_login_token(&LoginToken {
            id: 0,
            user_id: user.id,
            timestamp: now_timestamp(),
            token: login_token,
        }) {
            return self.db_fail(&e);
        }

        self.clients[i].login_data = Some(LoginData {
            user_id: user.id,
            steam_id,
            name: name.to_string(),
            password_hash: password_hash.to_string(),
            login_token,
        });
        self.clients[i].state = ClientState::LoggedIn;

        Self::slog("processPacket", "Successfully logged in\n");
        self.send_login_success(i, login_token, &user.name)
    }

    /// C++ `CTSPLogout` arm (`HexagonServer.cpp:1058-1082`): the
    /// `LoggedIn|LoggedIn_Ready` gate (wrong state → `true`, NO failure
    /// packet), the user lookup (missing → `STCPLogoutFailure`), the token
    /// removal, the session reset, and the encrypted success.
    fn handle_logout(&mut self, i: usize, steam_id: u64) -> bool {
        self.print_ctsp_data_verbose(i, "logout", &ClientToServer::CTSPLogout { steam_id });

        if !self.check_state2(i, ClientState::LoggedIn, ClientState::LoggedInReady) {
            return true;
        }

        let user = match self.db.get_user_with_steam_id(steam_id) {
            Ok(user) => user,
            Err(e) => return self.db_fail(&e),
        };

        match user {
            None => {
                Self::slog(
                    "processPacket",
                    &format!("No user with steamId '{steam_id}'\n"),
                );
                self.send_logout_failure(i)
            }
            Some(user) => {
                if let Err(e) = self.db.remove_all_login_tokens_for_user(user.id) {
                    return self.db_fail(&e);
                }
                self.clients[i].login_data = None;
                self.clients[i].state = ClientState::Connected;
                self.send_logout_success(i)
            }
        }
    }

    /// C++ `CTSPDeleteAccount` arm (`HexagonServer.cpp:1084-1134`): the
    /// connected gate (wrong state → `true`, NO failure packet), the
    /// existence + raw-byte password checks, the token removal +
    /// `removeUser`, and the encrypted success/failure.
    fn handle_delete_account(&mut self, i: usize, steam_id: u64, password_hash: &str) -> bool {
        self.print_ctsp_data_verbose(
            i,
            "delete account",
            &ClientToServer::CTSPDeleteAccount {
                steam_id,
                password_hash: password_hash.to_string(),
            },
        );

        if !self.check_state(i, ClientState::Connected) {
            return true;
        }

        if let Ok(any) = self.db.any_user_with_steam_id(steam_id) {
            if !any {
                let error = format!("No user with steamId '{steam_id}' registered");
                self.log_send_fail(&error);
                return self.send_delete_account_failure(i, &error);
            }
        } else if let Err(e) = self.db.any_user_with_steam_id(steam_id) {
            return self.db_fail(&e);
        }

        let user = match self.db.get_user_with_steam_id(steam_id) {
            Ok(user) => user,
            Err(e) => return self.db_fail(&e),
        };

        let user = match user {
            Some(user) => user,
            None => {
                let error = format!("No user with steamId '{steam_id}'");
                self.log_send_fail(&error);
                return self.send_delete_account_failure(i, &error);
            }
        };

        if user.password_hash != password_hash.as_bytes() {
            let error = format!("Invalid password for user matching '{steam_id}'");
            self.log_send_fail(&error);
            return self.send_delete_account_failure(i, &error);
        }

        if let Err(e) = self.db.remove_all_login_tokens_for_user(user.id) {
            return self.db_fail(&e);
        }
        if let Err(e) = self.db.remove_user(user.id) {
            return self.db_fail(&e);
        }

        Self::slog("processPacket", "Successfully deleted account\n");
        self.send_delete_account_success(i)
    }

    /// C++ `CTSPRequestTopScores` arm (`HexagonServer.cpp:1126-1161`):
    /// the `LoggedIn_Ready` + token gate, the level gate, the verbose
    /// line, then `sendTopScores` over the top `topScoresLimit` rows.
    fn handle_top_scores(&mut self, i: usize, login_token: u64, level_validator: &str) -> bool {
        self.print_ctsp_data_verbose(
            i,
            "request top scores",
            &ClientToServer::CTSPRequestTopScores {
                login_token,
                level_validator: level_validator.to_string(),
            },
        );

        if !self.check_state(i, ClientState::LoggedInReady)
            || !self.validate_login(i, "top scores", login_token)
        {
            return true;
        }

        if !self.is_level_supported(level_validator) {
            return true;
        }

        let scores = match self.db.get_top_scores(TOP_SCORES_LIMIT, level_validator) {
            Ok(scores) => scores,
            Err(e) => return self.db_fail(&e),
        };

        self.slog_verbose(
            "processPacket",
            &format!(
                "Sending top {} scores to client '{}'\n",
                TOP_SCORES_LIMIT, self.clients[i].client_id
            ),
        );
        self.send_top_scores(i, level_validator, &scores)
    }

    /// C++ `CTSPRequestOwnScore` arm (`HexagonServer.cpp:1163-1190`):
    /// the same gates, then `getScore(lv, loginData.steamId)` — no row →
    /// `true` (no packet), else `sendOwnScore`.
    fn handle_own_score(&mut self, i: usize, login_token: u64, level_validator: &str) -> bool {
        self.print_ctsp_data_verbose(
            i,
            "request own score",
            &ClientToServer::CTSPRequestOwnScore {
                login_token,
                level_validator: level_validator.to_string(),
            },
        );

        if !self.check_state(i, ClientState::LoggedInReady)
            || !self.validate_login(i, "own score", login_token)
        {
            return true;
        }

        if !self.is_level_supported(level_validator) {
            return true;
        }

        // C++ `c._loginData->_steamId` (the gate guarantees the login data).
        let steam_id = self.clients[i]
            .login_data
            .as_ref()
            .expect("the gate validated the login")
            .steam_id;
        let ps = match self.db.get_score(level_validator, steam_id) {
            Ok(ps) => ps,
            Err(e) => return self.db_fail(&e),
        };

        let ps = match ps {
            Some(ps) => ps,
            None => return true,
        };

        self.slog_verbose(
            "processPacket",
            &format!(
                "Sending own score to client '{}'\n",
                self.clients[i].client_id
            ),
        );
        self.send_own_score(i, level_validator, &ps)
    }

    /// C++ `CTSPRequestTopScoresAndOwnScore` arm
    /// (`HexagonServer.cpp:1192-1216`): the same gates, both DB queries
    /// (the C++'s argument order: `getTopScores` then `getScore`), then
    /// `sendTopScoresAndOwnScore`.
    fn handle_top_scores_and_own_score(
        &mut self,
        i: usize,
        login_token: u64,
        level_validator: &str,
    ) -> bool {
        self.print_ctsp_data_verbose(
            i,
            "request top scores and own score",
            &ClientToServer::CTSPRequestTopScoresAndOwnScore {
                login_token,
                level_validator: level_validator.to_string(),
            },
        );

        if !self.check_state(i, ClientState::LoggedInReady)
            || !self.validate_login(i, "top scores and own scores", login_token)
        {
            return true;
        }

        if !self.is_level_supported(level_validator) {
            return true;
        }

        let steam_id = self.clients[i]
            .login_data
            .as_ref()
            .expect("the gate validated the login")
            .steam_id;
        let (top, own) = match (
            self.db.get_top_scores(TOP_SCORES_LIMIT, level_validator),
            self.db.get_score(level_validator, steam_id),
        ) {
            (Ok(top), Ok(own)) => (top, own),
            (Err(e), _) | (_, Err(e)) => return self.db_fail(&e),
        };

        self.slog_verbose(
            "processPacket",
            &format!(
                "Sending top {} scores and own score to client '{}'\n",
                TOP_SCORES_LIMIT, self.clients[i].client_id
            ),
        );
        self.send_top_scores_and_own_score(i, level_validator, &top, &own)
    }

    /// C++ `CTSPStartedGame` arm (`HexagonServer.cpp:1218-1245`): the
    /// gate, the log, and the `_gameStatus` update (`_startTP` = the
    /// system clock, `_levelValidator` = the packet's).
    fn handle_started_game(&mut self, i: usize, login_token: u64, level_validator: &str) -> bool {
        self.print_ctsp_data_verbose(
            i,
            "started game",
            &ClientToServer::CTSPStartedGame {
                login_token,
                level_validator: level_validator.to_string(),
            },
        );

        if !self.check_state(i, ClientState::LoggedInReady)
            || !self.validate_login(i, "started game", login_token)
        {
            return true;
        }

        Self::slog(
            "processPacket",
            &format!(
                "Client '{}' started game for level '{}'\n",
                self.clients[i].client_id, level_validator
            ),
        );

        self.clients[i].game_status = Some(GameStatus {
            start_tp: SystemTime::now(),
            level_validator: level_validator.to_string(),
        });
        true
    }

    /// C++ `CTSPRequestServerStatus` arm (`HexagonServer.cpp:1267-1279`):
    /// the `LoggedIn` (NOT `LoggedIn_Ready`) + token gate, then
    /// `sendServerStatus`.
    fn handle_request_server_status(&mut self, i: usize, login_token: u64) -> bool {
        self.print_ctsp_data_verbose(
            i,
            "request server status",
            &ClientToServer::CTSPRequestServerStatus { login_token },
        );

        if !self.check_state(i, ClientState::LoggedIn)
            || !self.validate_login(i, "request server status", login_token)
        {
            return true;
        }

        self.send_server_status(i)
    }

    /// C++ `CTSPReady` arm (`HexagonServer.cpp:1281-1293`): the
    /// `LoggedIn` + token gate, then the state flip.
    fn handle_ready(&mut self, i: usize, login_token: u64) -> bool {
        self.print_ctsp_data_verbose(i, "ready", &ClientToServer::CTSPReady { login_token });

        if !self.check_state(i, ClientState::LoggedIn)
            || !self.validate_login(i, "ready", login_token)
        {
            return true;
        }

        self.clients[i].state = ClientState::LoggedInReady;
        true
    }

    /// C++ `processReplay`'s arm-local `discard` lambda
    /// (`HexagonServer.cpp:650-658`): the exact log line (the client addr,
    /// the reason, `rf.played_seconds()`), then `true`.
    fn discard_replay(&self, i: usize, reason: &str, rf: &oh_replay::ReplayFile) -> bool {
        Self::slog(
            "processReplay",
            &format!(
                "Discarding replay from client '{}', {}, replay time was {}s\n",
                self.clients[i].client_id,
                reason,
                rf.played_seconds()
            ),
        );
        true
    }

    /// C++ `processReplay` (`HexagonServer.cpp:642-747`): the receive
    /// timestamp, the token gate, the discard chain (no game started /
    /// invalid pack id / invalid level id / unscored level / max
    /// processing time / difference too large / bad ratio), the replay
    /// execution over the [`ReplayExecutor`] seam (the C++'s
    /// `_hexagonGame.runReplayUntilDeathAndGetScore(rf, 5, 1.f)`), the
    /// elapsed/difference/ratio checks (the C++'s signed
    /// `receiveTime - startTP`), and the `addScore` (the C++'s
    /// `nowTimestamp()` + `loginData.steamId` + `replayedTime`).
    fn process_replay(&mut self, i: usize, login_token: u64, rf: &oh_replay::ReplayFile) -> bool {
        let receive_time = SystemTime::now();

        if !self.validate_login(i, "replay", login_token) {
            return true;
        }

        let game_status = match self.clients[i].game_status.as_ref() {
            Some(game_status) => game_status,
            None => return self.discard_replay(i, "no game started", rf),
        };

        if !self.assets.is_valid_pack_id(&rf.pack_id) {
            return self.discard_replay(i, &format!("invalid pack id '{}'", rf.pack_id), rf);
        }

        if !self.assets.is_valid_level_id(&rf.level_id) {
            return self.discard_replay(i, &format!("invalid level id '{}'", rf.level_id), rf);
        }

        // C++ `_assets.getLevelData(rf._level_id)` (its `assert` on the
        // validity — checked above).
        let level_data = self
            .assets
            .get_level_data(&rf.level_id)
            .expect("the is_valid_level_id above");

        if level_data.unscored {
            return self.discard_replay(i, &format!("unscored level id '{}'", rf.level_id), rf);
        }

        // C++ `Utils::getLevelValidator(rf._level_id, rf._difficulty_mult)` —
        // the raw `"{levelId}_m_{dm}"` construction (NOT a supported-
        // validators lookup — the C++'s `processReplay` never
        // `isLevelSupported`s).
        let level_validator = oh_data::get_level_validator(&rf.level_id, rf.difficulty_mult);

        Self::slog(
            "processReplay",
            &format!(
                "Processing replay from client '{}' for level '{}'\n",
                self.clients[i].client_id, level_validator
            ),
        );

        // C++ local `maxProcessingSeconds = 5` (`:681`).
        const MAX_PROCESSING_SECONDS: i32 = 5;

        let ger = self.replay_executor.run_replay_until_death_and_get_score(
            rf,
            MAX_PROCESSING_SECONDS,
            1.0,
        );
        let ger = match ger {
            Some(ger) => ger,
            None => {
                return self.discard_replay(
                    i,
                    &format!("max processing time exceeded ({MAX_PROCESSING_SECONDS}s)"),
                    rf,
                )
            }
        };

        let replay_total_time = ger.total_time_seconds;
        let replay_played_time = ger.played_time_seconds;

        Self::slog(
            "processReplay",
            &format!("Replay processed, final time: '{replay_total_time}'\n"),
        );

        // C++ `receiveTime - c._gameStatus->_startTP` — SIGNED (a future
        // `startTP` yields a negative elapsed).
        let elapsed_secs: f64 = receive_time
            .duration_since(game_status.start_tp)
            .map(|d| d.as_secs_f64())
            .unwrap_or_else(|e| -e.duration().as_secs_f64());

        let difference = (replay_total_time - elapsed_secs).abs();
        let ratio = replay_total_time / elapsed_secs;

        let good_difference = difference < 5.0;
        let good_ratio = ratio > 0.65 && ratio < 1.35;

        let print_difference_and_ratio = || {
            Self::slog(
                "processReplay",
                &format!(
                    "Elapsed request time: {elapsed_secs}\nDifference: {difference}\nRatio: {ratio}\n"
                ),
            );
        };

        if !good_difference {
            print_difference_and_ratio();
            return self.discard_replay(i, "difference too large", rf);
        }

        if !good_ratio {
            print_difference_and_ratio();
            return self.discard_replay(i, "bad ratio", rf);
        }

        Self::slog("processReplay", "Replay valid, adding to database\n");

        // C++ `c._loginData->_steamId` (the token gate guarantees it).
        let steam_id = self.clients[i]
            .login_data
            .as_ref()
            .expect("the token gate validated the login")
            .steam_id;
        if let Err(e) = self.db.add_score(
            &level_validator,
            now_timestamp(),
            steam_id,
            replay_played_time,
        ) {
            return self.db_fail(&e);
        }

        true
    }

    /// C++ `CTSPReplay` arm (`HexagonServer.cpp:1149-1157`): the verbose
    /// print, the `LoggedIn_Ready` gate, then `processReplay`.
    fn handle_replay(&mut self, i: usize, ctsp: ClientToServer) -> bool {
        self.print_ctsp_data_verbose(i, "replay", &ctsp);

        let ClientToServer::CTSPReplay {
            login_token,
            replay_file,
        } = ctsp
        else {
            unreachable!("the arm passes only CTSPReplay")
        };

        if !self.check_state(i, ClientState::LoggedInReady) {
            return true;
        }

        self.process_replay(i, login_token, &replay_file)
    }

    /// C++ `CTSPCompressedReplay` arm (`HexagonServer.cpp:1246-1264`): the
    /// verbose print, the `LoggedIn_Ready` gate, the `decompressReplayFile`
    /// (failure → the error line + `false` — the failure counter bumps),
    /// then `processReplay`.
    fn handle_compressed_replay(&mut self, i: usize, ctsp: ClientToServer) -> bool {
        self.print_ctsp_data_verbose(i, "compressed replay", &ctsp);

        let ClientToServer::CTSPCompressedReplay {
            login_token,
            compressed_replay_file,
        } = ctsp
        else {
            unreachable!("the arm passes only CTSPCompressedReplay")
        };

        if !self.check_state(i, ClientState::LoggedInReady) {
            return true;
        }

        let rf = match oh_replay::decompress_replay_file(&compressed_replay_file) {
            Some(rf) => rf,
            None => {
                Self::slog_error(
                    "processPacket",
                    &format!(
                        "Failed to decompress replay received from client '{}'\n",
                        self.clients[i].client_id
                    ),
                );
                return false;
            }
        };

        self.process_replay(i, login_token, &rf)
    }

    /// C++ `checkState2` lambda (`HexagonServer.cpp:833-845`).
    fn check_state2(&self, i: usize, state0: ClientState, state1: ClientState) -> bool {
        if self.clients[i].state != state0 && self.clients[i].state != state1 {
            self.slog_verbose(
                "processPacket",
                &format!(
                    "Invalid client state, expected '{}' or '{}', state was '{}''\n",
                    state0.as_int(),
                    state1.as_int(),
                    self.clients[i].state.as_int()
                ),
            );
            return false;
        }
        true
    }

    /// C++ `processPacket` (`HexagonServer.cpp:809-1296`): the decode +
    /// the per-type arms. `i` = the client's index (the port's split-borrow
    /// regime — the C++ passes the `ConnectedClient&`); `true` = the packet
    /// was handled successfully (the caller refreshes `_lastActivity` +
    /// resets `_consecutiveFailures`).
    fn process_packet(&mut self, i: usize, p: &mut Packet) -> bool {
        // C++ `_errorOss.str("")` (the pre-populated content is preserved
        // by the decode — `shared.rs`'s `error_oss` contract).
        self.error_oss.clear();
        let key = self.clients[i].rt_keys.as_ref().map(|k| &k.key_receive);
        let pv = decode_client_to_server_packet(key, p, &mut self.error_oss);

        match pv {
            ClientToServer::PInvalid { .. } => {
                // The C++'s `PInvalid` arm (the decode's failure model).
                self.fail(
                    "processPacket",
                    &format!(
                        "Error processing packet from client '{}', details: {}",
                        self.clients[i].client_id, self.error_oss
                    ),
                )
            }
            ClientToServer::PEncryptedMsg { .. } => {
                // Defensive (the decode returns `PInvalid` on the
                // encrypted path's failures — the C++'s arm is
                // equally defensive).
                self.fail(
                    "processPacket",
                    &format!(
                        "Received non-decrypted encrypted msg packet from client '{}'",
                        self.clients[i].client_id
                    ),
                )
            }
            ClientToServer::CTSPHeartbeat => true,
            ClientToServer::CTSPDisconnect => {
                // C++ (`HexagonServer.cpp:880-888`): `_mustDisconnect` +
                // the state flip.
                self.clients[i].must_disconnect = true;
                self.clients[i].state = ClientState::Disconnected;
                true
            }
            // --------------------------------------------------------
            // The replay arms (the milestone-4 port of the C++'s
            // `processReplay`, `HexagonServer.cpp:642-747`).
            ClientToServer::CTSPPublicKey { key } => self.handle_public_key(i, key),
            ClientToServer::CTSPRegister {
                steam_id,
                name,
                password_hash,
            } => self.handle_register(i, steam_id, &name, &password_hash),
            ClientToServer::CTSPLogin {
                steam_id,
                name,
                password_hash,
            } => self.handle_login(i, steam_id, &name, &password_hash),
            ClientToServer::CTSPLogout { steam_id } => self.handle_logout(i, steam_id),
            ClientToServer::CTSPDeleteAccount {
                steam_id,
                password_hash,
            } => self.handle_delete_account(i, steam_id, &password_hash),
            ClientToServer::CTSPRequestTopScores {
                login_token,
                level_validator,
            } => self.handle_top_scores(i, login_token, &level_validator),
            v @ ClientToServer::CTSPReplay { .. } => self.handle_replay(i, v),
            ClientToServer::CTSPRequestOwnScore {
                login_token,
                level_validator,
            } => self.handle_own_score(i, login_token, &level_validator),
            ClientToServer::CTSPRequestTopScoresAndOwnScore {
                login_token,
                level_validator,
            } => self.handle_top_scores_and_own_score(i, login_token, &level_validator),
            ClientToServer::CTSPStartedGame {
                login_token,
                level_validator,
            } => self.handle_started_game(i, login_token, &level_validator),
            v @ ClientToServer::CTSPCompressedReplay { .. } => self.handle_compressed_replay(i, v),
            ClientToServer::CTSPRequestServerStatus { login_token } => {
                self.handle_request_server_status(i, login_token)
            }
            ClientToServer::CTSPReady { login_token } => self.handle_ready(i, login_token),
        }
    }
}

impl Drop for HexagonServer {
    /// C++ `~HexagonServer` (`HexagonServer.cpp:1413-1428`): kick every
    /// remaining client, close everything. (The C++ sets each socket
    /// blocking for the final kick — the port's non-blocking kick is the
    /// equivalent; the sockets close on the field drops.)
    fn drop(&mut self) {
        Self::slog(
            "HexagonServer::~HexagonServer",
            "Uninitializing server...\n",
        );
        let n = self.clients.len();
        for i in 0..n {
            let mut p = Packet::new();
            make_server_to_client_packet(&mut p, &ServerToClient::STCPKick);
            let payload = p.data();
            if let Some(c) = self.clients.get_mut(i) {
                let _ = Self::send_frame(c, payload);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests (the milestone-1 smoke: the skeleton's decode/dispatch + the
// socket iteration)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;

    use super::*;
    use std::io::Write;
    use std::path::Path;

    /// A `ReplayExecutor` stand-in (the C++'s `HexagonGame&` — no replay
    /// processing exists yet in milestone 1).
    struct NullExecutor;

    impl ReplayExecutor for NullExecutor {
        fn run_replay_until_death_and_get_score(
            &self,
            _rf: &oh_replay::ReplayFile,
            _max_processing_seconds: i32,
            _timescale: f32,
        ) -> Option<GameExecutionResult> {
            None
        }
    }

    /// The scripted [`ReplayExecutor`] for the replay tests: the C++'s
    /// `HexagonGame` is the composition's (the port's seam — the
    /// `ReplayExecutor` trait); the outcomes are keyed on the replay's
    /// `player_name` (a field the C++'s real executor never reads — the
    /// seam, not the game logic). The call counter pins which discards
    /// happen BEFORE the executor runs.
    struct ScriptExecutor {
        calls: Arc<AtomicUsize>,
    }

    impl ReplayExecutor for ScriptExecutor {
        fn run_replay_until_death_and_get_score(
            &self,
            rf: &oh_replay::ReplayFile,
            _max_processing_seconds: i32,
            _timescale: f32,
        ) -> Option<GameExecutionResult> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let outcome = |total: f64| GameExecutionResult {
                total_time_seconds: total,
                played_time_seconds: total,
                paused_time_seconds: 0.0,
                custom_score: 0.0,
            };
            Some(match rf.player_name.as_str() {
                // The valid path: total ≈ the test's pinned 2 s
                // `startTP` age (ratio ≈ 1, difference < 5) — the played
                // time (the `addScore` value) is 42.0, NOT the total.
                "GOOD" => {
                    return Some(GameExecutionResult {
                        total_time_seconds: 2.0,
                        played_time_seconds: 42.0,
                        paused_time_seconds: 0.0,
                        custom_score: 0.0,
                    })
                }
                // The "difference too large" discard: |100.0 - ~2.0| > 5.
                "BADTIME" => return Some(outcome(100.0)),
                // The "bad ratio" discard: 0.0 / ~2.0 = 0.0 < 0.65 (the
                // difference is ~2.0 < 5 — so the ratio gate, not the
                // difference gate, is what fires).
                "BADRATIO" => return Some(outcome(0.0)),
                // The C++'s `std::nullopt` — the max-processing-time
                // discard.
                "SLOW" => return None,
                _ => outcome(1.0),
            })
        }
    }

    /// The test composition: a temp DB + the `_RELEASE` asset root, ports
    /// 0 (OS-assigned).
    fn make_server(dir: &tempfile::TempDir) -> HexagonServer {
        make_server_with_executor(dir, Box::new(NullExecutor))
    }

    fn make_server_with_executor(
        dir: &tempfile::TempDir,
        executor: Box<dyn ReplayExecutor>,
    ) -> HexagonServer {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../_RELEASE");
        let assets =
            Arc::new(oh_assets::Assets::new(&root, true, false).expect("_RELEASE loads headless"));
        let db = Database::open(&dir.path().join("ohdb.sqlite")).expect("temp db");
        HexagonServer::new(
            assets,
            executor,
            db,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            0,
            0,
            HashSet::from(["ohvrvanilla_vittorio_romeo_cube_1_pi_m_1".to_string()]),
        )
        .expect("server binds")
    }

    /// The C++'s wire frame: u32-BE size + payload.
    fn frame_bytes(packet: &Packet) -> Vec<u8> {
        let payload = packet.data();
        let mut frame = Vec::with_capacity(4 + payload.len());
        frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        frame.extend_from_slice(payload);
        frame
    }

    fn send_frame(stream: &mut TcpStream, packet: &Packet) {
        stream.write_all(&frame_bytes(packet)).expect("send");
    }

    /// N server iterations (each consumes at most one client frame).
    fn spin(server: &mut HexagonServer, n: u32) {
        for _ in 0..n {
            server.run_iteration();
        }
    }

    /// The raw client side of the auth tests: a real `TcpStream`, a
    /// FIXED PS keypair (the wire-test's `keypair_from_seed` pattern —
    /// the server's keys stay random; the handshake itself is the pin),
    /// and the client-side RT keys (computed from the server's
    /// `STCPPublicKey`).
    struct AuthClient {
        stream: TcpStream,
        ps_keys: crate::sodium::SodiumPsKeys,
        rt_keys: Option<crate::sodium::SodiumRtKeys>,
    }

    impl AuthClient {
        fn connect(port: u16, seed_byte: u8) -> Self {
            let (public_key, secret_key) = sodiumoxide::crypto::kx::keypair_from_seed(
                &sodiumoxide::crypto::kx::Seed([seed_byte; 32]),
            );
            let ps_keys = crate::sodium::SodiumPsKeys {
                key_public: public_key.as_ref().try_into().expect("32 bytes"),
                key_secret: secret_key.as_ref().try_into().expect("32 bytes"),
            };
            let stream = TcpStream::connect(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port))
                .expect("connect");
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("read timeout");
            Self {
                stream,
                ps_keys,
                rt_keys: None,
            }
        }

        fn send(&mut self, packet: &Packet) {
            self.stream
                .write_all(&frame_bytes(packet))
                .expect("client send");
        }

        fn send_plain(&mut self, cts: &ClientToServer) {
            let mut p = Packet::new();
            crate::shared::make_client_to_server_packet(&mut p, cts);
            self.send(&p);
        }

        fn send_encrypted(&mut self, cts: &ClientToServer) {
            let rt = self.rt_keys.expect("the handshake must have run");
            let mut p = Packet::new();
            crate::shared::make_client_to_server_encrypted_packet(&rt.key_transmit, &mut p, cts);
            self.send(&p);
        }

        /// One wire frame (u32-BE size + payload — the C++'s `receive`).
        fn read_frame(&mut self) -> Packet {
            use std::io::Read;
            let mut size_buf = [0u8; 4];
            self.stream.read_exact(&mut size_buf).expect("frame size");
            let size = u32::from_be_bytes(size_buf) as usize;
            let mut buf = vec![0u8; size];
            self.stream.read_exact(&mut buf).expect("frame payload");
            let mut p = Packet::new();
            p.append(&buf);
            p
        }

        fn read_plain(&mut self) -> ServerToClient {
            let mut p = self.read_frame();
            let mut error = String::new();
            crate::shared::decode_server_to_client_packet(None, &mut p, &mut error)
        }

        fn read_encrypted(&mut self) -> ServerToClient {
            let rt = self.rt_keys.expect("the handshake must have run");
            let mut p = self.read_frame();
            let mut error = String::new();
            crate::shared::decode_server_to_client_packet(Some(&rt.key_receive), &mut p, &mut error)
        }

        /// The C++ client's exact `saltAndHashPwd` value
        /// (`HexagonClient.cpp:799-811`): the raw `sodiumHash("salt" +
        /// password)` digest, the non-UTF-8 bytes wrapped unchecked (the
        /// `client.rs:604-613` precedent).
        fn salted_hash(password: &str) -> String {
            let digest = crate::sodium::sodium_hash(format!("salt{password}").as_bytes());
            assert!(
                std::str::from_utf8(&digest).is_err(),
                "the fixture digest must NOT be valid UTF-8 (the raw-byte path)"
            );
            unsafe { String::from_utf8_unchecked(digest.to_vec()) }
        }

        /// The C++ gate paths (`validateLogin`/`isLevelSupported`/no own
        /// score row) send NO packet at all — assert a short window passes
        /// with no frame bytes (the arms' `return true`).
        fn expect_no_frame(&mut self) {
            use std::io::Read;
            self.stream
                .set_read_timeout(Some(Duration::from_millis(300)))
                .expect("timeout");
            let mut buf = [0u8; 1];
            let res = self.stream.read(&mut buf);
            match res {
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                other => panic!("expected no frame, got {other:?}"),
            }
            self.stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("timeout");
        }

        /// The milestone-2/3 common prefix: the PS-key handshake, the
        /// registration (the C++'s exact salted digest), the login —
        /// returning the client + the issued login token.
        fn register_and_login(
            server: &mut HexagonServer,
            port: u16,
            seed_byte: u8,
            steam_id: u64,
            name: &str,
        ) -> (Self, u64) {
            let mut client = Self::connect(port, seed_byte);
            client.send_plain(&ClientToServer::CTSPPublicKey {
                key: client.ps_keys.key_public,
            });
            spin(server, 5);
            match client.read_plain() {
                ServerToClient::STCPPublicKey { key } => {
                    let rt = crate::sodium::calculate_client_session_sodium_rt_keys(
                        &client.ps_keys,
                        &key,
                    )
                    .expect("the client RT keys");
                    client.rt_keys = Some(rt);
                }
                other => panic!("expected STCPPublicKey, got {other:?}"),
            }
            let pwhash = Self::salted_hash("password");
            client.send_encrypted(&ClientToServer::CTSPRegister {
                steam_id,
                name: name.to_string(),
                password_hash: pwhash.clone(),
            });
            spin(server, 5);
            assert!(
                matches!(
                    client.read_encrypted(),
                    ServerToClient::STCPRegistrationSuccess
                ),
                "the registration"
            );
            client.send_encrypted(&ClientToServer::CTSPLogin {
                steam_id,
                name: name.to_string(),
                password_hash: pwhash,
            });
            spin(server, 5);
            let token = match client.read_encrypted() {
                ServerToClient::STCPLoginSuccess {
                    login_token,
                    login_name,
                } => {
                    assert_eq!(login_name, name);
                    login_token
                }
                other => panic!("expected STCPLoginSuccess, got {other:?}"),
            };
            (client, token)
        }
    }

    /// The milestone-2 flow over a real socket: the PS-key handshake,
    /// registration (the raw 32-byte digest — the C++'s exact hashing),
    /// the duplicate/unregistered/wrong-password failures (the C++'s exact
    /// error lines), the login + token issuance, the logout + token
    /// removal, and the delete-account success/failure.
    #[test]
    fn the_auth_flow_register_login_logout_delete_account() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut server = make_server(&dir);

        let mut client = AuthClient::connect(server.port(), 0x2A);

        // -- the handshake (the C++'s CTSPPublicKey arm + sendPublicKey)
        client.send_plain(&ClientToServer::CTSPPublicKey {
            key: client.ps_keys.key_public,
        });
        spin(&mut server, 5);
        match client.read_plain() {
            ServerToClient::STCPPublicKey { key } => {
                let rt =
                    crate::sodium::calculate_client_session_sodium_rt_keys(&client.ps_keys, &key)
                        .expect("the client RT keys");
                client.rt_keys = Some(rt);
            }
            other => panic!("expected STCPPublicKey, got {other:?}"),
        }
        assert!(
            server.clients[0].rt_keys.is_some(),
            "the server stored the RT keys"
        );
        assert_eq!(
            server.clients[0].client_public_key,
            Some(client.ps_keys.key_public),
            "the server stored the client's public key"
        );

        // -- registration: the raw 32-byte digest (NOT valid UTF-8 — the
        // C++'s `std::string` carries it raw; the decode side must too).
        let pwhash = AuthClient::salted_hash("password");
        client.send_encrypted(&ClientToServer::CTSPRegister {
            steam_id: 4242,
            name: "tester".to_string(),
            password_hash: pwhash.clone(),
        });
        spin(&mut server, 5);
        assert!(
            matches!(
                client.read_encrypted(),
                ServerToClient::STCPRegistrationSuccess
            ),
            "the encrypted registration success"
        );

        let users = server.db().dump_users().expect("users");
        assert_eq!(users.len(), 1, "exactly one user row");
        assert_eq!(users[0].steam_id, 4242);
        assert_eq!(
            users[0].password_hash,
            crate::sodium::sodium_hash(b"saltpassword").to_vec(),
            "the raw digest bytes were stored (the C++ `stringToCharVec`)"
        );

        // -- duplicate registration → the C++'s error line
        client.send_encrypted(&ClientToServer::CTSPRegister {
            steam_id: 4242,
            name: "tester".to_string(),
            password_hash: pwhash.clone(),
        });
        spin(&mut server, 5);
        match client.read_encrypted() {
            ServerToClient::STCPRegistrationFailure { error } => {
                assert_eq!(error, "User with steamId '4242' already registered");
            }
            other => panic!("expected STCPRegistrationFailure, got {other:?}"),
        }

        // -- login with the WRONG password → the C++'s error line
        let wrong = AuthClient::salted_hash("WRONG");
        client.send_encrypted(&ClientToServer::CTSPLogin {
            steam_id: 4242,
            name: "tester".to_string(),
            password_hash: wrong.clone(),
        });
        spin(&mut server, 5);
        match client.read_encrypted() {
            ServerToClient::STCPLoginFailure { error } => {
                assert_eq!(
                    error,
                    "Invalid password for user matching '4242' and 'tester'"
                );
            }
            other => panic!("expected STCPLoginFailure, got {other:?}"),
        }

        // -- login with the RIGHT password → the token + the stored name
        client.send_encrypted(&ClientToServer::CTSPLogin {
            steam_id: 4242,
            name: "tester".to_string(),
            password_hash: pwhash.clone(),
        });
        spin(&mut server, 5);
        let token = match client.read_encrypted() {
            ServerToClient::STCPLoginSuccess {
                login_token,
                login_name,
            } => {
                assert_eq!(login_name, "tester", "the C++ sends the stored name");
                login_token
            }
            other => panic!("expected STCPLoginSuccess, got {other:?}"),
        };
        assert_eq!(server.clients[0].state, ClientState::LoggedIn);
        assert!(
            server
                .db()
                .is_login_token_valid(token)
                .expect("the token row"),
            "the issued token is valid in the DB"
        );

        // -- a registration while LoggedIn → the C++ state-gate line
        // (the encrypted failure)
        client.send_encrypted(&ClientToServer::CTSPRegister {
            steam_id: 4243,
            name: "x".to_string(),
            password_hash: pwhash.clone(),
        });
        spin(&mut server, 5);
        match client.read_encrypted() {
            ServerToClient::STCPRegistrationFailure { error } => {
                assert_eq!(error, "Client not in connected state");
            }
            other => panic!("expected STCPRegistrationFailure, got {other:?}"),
        }

        // -- logout → the success + the session reset + the token removal
        client.send_encrypted(&ClientToServer::CTSPLogout { steam_id: 4242 });
        spin(&mut server, 5);
        assert!(
            matches!(client.read_encrypted(), ServerToClient::STCPLogoutSuccess),
            "the encrypted logout success"
        );
        assert_eq!(
            server.clients[0].state,
            ClientState::Connected,
            "the state reset"
        );
        assert!(
            server.clients[0].login_data.is_none(),
            "the session login data cleared"
        );
        assert!(
            !server
                .db()
                .is_login_token_valid(token)
                .expect("the token row"),
            "the logout removed the user's tokens (the C++ removeAllLoginTokensForUser)"
        );

        // -- delete account with the WRONG password → the C++'s line
        client.send_encrypted(&ClientToServer::CTSPDeleteAccount {
            steam_id: 4242,
            password_hash: wrong,
        });
        spin(&mut server, 5);
        match client.read_encrypted() {
            ServerToClient::STCPDeleteAccountFailure { error } => {
                assert_eq!(error, "Invalid password for user matching '4242'");
            }
            other => panic!("expected STCPDeleteAccountFailure, got {other:?}"),
        }

        // -- delete account with the RIGHT password → the row is gone
        client.send_encrypted(&ClientToServer::CTSPDeleteAccount {
            steam_id: 4242,
            password_hash: pwhash,
        });
        spin(&mut server, 5);
        assert!(
            matches!(
                client.read_encrypted(),
                ServerToClient::STCPDeleteAccountSuccess
            ),
            "the encrypted delete-account success"
        );
        assert!(
            server.db().dump_users().expect("users").is_empty(),
            "the user row was removed (the C++ removeUser)"
        );
    }

    /// The milestone-3 flow over a real socket: the server-status +
    /// ready pair (the `LoggedIn` gates), the top/own-score packets
    /// (value-descending, the 0-based positions, the own-score absence →
    /// no packet), the `validateLogin`/`isLevelSupported` gates (no
    /// packet), and the `CTSPStartedGame` game-status update.
    #[test]
    fn the_scores_status_ready_flow() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut server = make_server(&dir);
        let lv = "ohvrvanilla_vittorio_romeo_cube_1_pi_m_1";

        // Seed the score board (the C++ DB shape: the scores INNER-JOIN
        // onto the users rows via steamId).
        for (sid, name, value) in [(71, "alpha", 10.5), (72, "beta", 42.0), (73, "gamma", 3.25)] {
            server
                .db_mut()
                .add_user(&User {
                    id: 0,
                    steam_id: sid,
                    name: name.into(),
                    password_hash: vec![0u8; 32],
                })
                .expect("seed user");
            server
                .db_mut()
                .add_score(lv, 1_700_000_000, sid, value)
                .expect("seed score");
        }

        let port = server.port();
        let (mut client, token) =
            AuthClient::register_and_login(&mut server, port, 0x3B, 4242, "tester");

        // -- CTSPRequestServerStatus (the `LoggedIn` gate): the
        // protocol/game versions + the supported validators.
        client.send_encrypted(&ClientToServer::CTSPRequestServerStatus { login_token: token });
        spin(&mut server, 5);
        match client.read_encrypted() {
            ServerToClient::STCPServerStatus {
                protocol_version,
                game_version,
                supported_level_validators,
            } => {
                assert_eq!(protocol_version, 0);
                assert_eq!(game_version, GAME_VERSION);
                assert_eq!(supported_level_validators, vec![lv.to_string()]);
            }
            other => panic!("expected STCPServerStatus, got {other:?}"),
        }

        // -- CTSPReady (the state flip, no packet).
        client.send_encrypted(&ClientToServer::CTSPReady { login_token: token });
        spin(&mut server, 5);
        client.expect_no_frame();
        assert_eq!(
            server.clients[0].state,
            ClientState::LoggedInReady,
            "the state flip"
        );

        // -- CTSPRequestTopScores: value-descending, positions 0..2.
        client.send_encrypted(&ClientToServer::CTSPRequestTopScores {
            login_token: token,
            level_validator: lv.to_string(),
        });
        spin(&mut server, 5);
        match client.read_encrypted() {
            ServerToClient::STCPTopScores {
                level_validator,
                scores,
            } => {
                assert_eq!(level_validator, lv);
                assert_eq!(scores.len(), 3);
                assert_eq!(scores[0].user_name, "beta");
                assert_eq!(scores[0].score_value, 42.0);
                assert_eq!(scores[0].position, 0);
                assert_eq!(scores[1].user_name, "alpha");
                assert_eq!(scores[1].score_value, 10.5);
                assert_eq!(scores[2].user_name, "gamma");
                assert_eq!(scores[2].score_value, 3.25);
                assert_eq!(scores[2].position, 2);
            }
            other => panic!("expected STCPTopScores, got {other:?}"),
        }

        // -- CTSPRequestOwnScore for a user with no score row: the C++
        // returns `true` (no packet).
        client.send_encrypted(&ClientToServer::CTSPRequestOwnScore {
            login_token: token,
            level_validator: lv.to_string(),
        });
        spin(&mut server, 5);
        client.expect_no_frame();

        // Seed our own score (7.75 — 42.0(0), 10.5(1), 7.75(2), 3.25(3)),
        // then re-request: position 2.
        server
            .db_mut()
            .add_score(lv, 1_700_000_001, 4242, 7.75)
            .expect("own score");
        client.send_encrypted(&ClientToServer::CTSPRequestOwnScore {
            login_token: token,
            level_validator: lv.to_string(),
        });
        spin(&mut server, 5);
        match client.read_encrypted() {
            ServerToClient::STCPOwnScore { score, .. } => {
                assert_eq!(score.user_name, "tester");
                assert_eq!(score.score_value, 7.75);
                assert_eq!(score.position, 2, "the value-descending position");
            }
            other => panic!("expected STCPOwnScore, got {other:?}"),
        }

        // -- CTSPRequestTopScoresAndOwnScore: both in one packet.
        client.send_encrypted(&ClientToServer::CTSPRequestTopScoresAndOwnScore {
            login_token: token,
            level_validator: lv.to_string(),
        });
        spin(&mut server, 5);
        match client.read_encrypted() {
            ServerToClient::STCPTopScoresAndOwnScore {
                scores, own_score, ..
            } => {
                assert_eq!(scores.len(), 4);
                assert_eq!(scores[2].user_name, "tester");
                assert_eq!(own_score.expect("the own score").user_name, "tester");
            }
            other => panic!("expected STCPTopScoresAndOwnScore, got {other:?}"),
        }

        // -- A wrong token: the C++ `validateLogin` gate (no packet).
        client.send_encrypted(&ClientToServer::CTSPRequestTopScores {
            login_token: token.wrapping_add(1),
            level_validator: lv.to_string(),
        });
        spin(&mut server, 5);
        client.expect_no_frame();

        // -- An unsupported level: the C++ `isLevelSupported` gate (no
        // packet) — even with the right token.
        client.send_encrypted(&ClientToServer::CTSPRequestTopScores {
            login_token: token,
            level_validator: "ohvrvanilla_unsupported".to_string(),
        });
        spin(&mut server, 5);
        client.expect_no_frame();

        // -- CTSPStartedGame: no packet; the game-status update.
        client.send_encrypted(&ClientToServer::CTSPStartedGame {
            login_token: token,
            level_validator: lv.to_string(),
        });
        spin(&mut server, 5);
        client.expect_no_frame();
        assert_eq!(
            server.clients[0]
                .game_status
                .as_ref()
                .map(|gs| gs.level_validator.clone()),
            Some(lv.to_string()),
            "the C++ _gameStatus update"
        );
    }

    /// The milestone-4 flow over a real socket: the `processReplay`
    /// (`HexagonServer.cpp:642-747`) discard chain (no game started /
    /// invalid pack / invalid level / unscored level — all BEFORE the
    /// executor; max processing time / difference too large / bad ratio —
    /// after it) and the valid path (the `addScore` under the pack-
    /// qualified `getLevelValidator(rf._level_id, ...)` key — the
    /// whitelist validator itself; the executor's PLAYED time is the
    /// stored value).
    #[test]
    fn the_replay_discard_paths_and_the_valid_add_score() {
        let dir = tempfile::tempdir().expect("tempdir");
        let calls = Arc::new(AtomicUsize::new(0));
        let mut server = make_server_with_executor(
            &dir,
            Box::new(ScriptExecutor {
                calls: calls.clone(),
            }),
        );
        let port = server.port();
        let (mut client, token) =
            AuthClient::register_and_login(&mut server, port, 0x4C, 4242, "tester");

        // Ready (the `processReplay` arms' `LoggedIn_Ready` gate).
        client.send_encrypted(&ClientToServer::CTSPReady { login_token: token });
        spin(&mut server, 5);
        client.expect_no_frame();
        assert_eq!(server.clients[0].state, ClientState::LoggedInReady);

        let cube_pack = "ohvrvanilla_vittorio_romeo_cube_1";
        // The PACK-QUALIFIED level id (the C++'s `levelDatas` key =
        // `concat(mPackId, '_', levelData.id)`, `Assets.cpp:989-992` —
        // what a replay's `_level_id` carries; the C++'s own
        // `runReplay` asserts `isValidLevelId(replay._level_id)` against
        // those keys). The whitelist validator is built from THIS id.
        let pi_level = "ohvrvanilla_vittorio_romeo_cube_1_pi";
        let tutorial_pack = "ohvrvanilla_vittorio_romeo_tutorial_1";
        // The C++'s `addScore` key: `getLevelValidator(rf._level_id, 1.0)`
        // = the pack-qualified validator (the whitelist entry itself).
        let stored_validator = "ohvrvanilla_vittorio_romeo_cube_1_pi_m_1";

        fn replay(player: &str, pack: &str, level: &str) -> oh_replay::ReplayFile {
            oh_replay::ReplayFile {
                version: 1,
                player_name: player.to_string(),
                seed: 0,
                data: oh_replay::ReplayData::default(),
                pack_id: pack.to_string(),
                level_id: level.to_string(),
                first_play: false,
                difficulty_mult: 1.0,
                played_score: 0.0,
            }
        }

        let assert_no_score = |server: &HexagonServer| {
            assert_eq!(
                server
                    .db()
                    .get_score(stored_validator, 4242)
                    .expect("score query"),
                None,
                "no score row"
            );
        };

        // -- A: no game started → the discard BEFORE the executor.
        client.send_encrypted(&ClientToServer::CTSPReplay {
            login_token: token,
            replay_file: replay("GOOD", cube_pack, pi_level),
        });
        spin(&mut server, 5);
        client.expect_no_frame();
        assert_eq!(calls.load(Ordering::SeqCst), 0, "the executor never ran");
        assert_no_score(&server);

        // The C++'s check runs against the WALL CLOCK (the elapsed
        // `receiveTime - startTP`); the in-module test pins the `startTP`
        // to 2 s in the past (the `CTSPStartedGame` arm itself is pinned
        // by `the_scores_status_ready_flow`). Each `expect_no_frame`
        // window advances the real clock by 300 ms, so the pin is
        // refreshed right before every timing-sensitive scenario.
        let pin_game_start = |server: &mut HexagonServer| {
            server.clients[0].game_status = Some(GameStatus {
                start_tp: SystemTime::now() - Duration::from_millis(2000),
                level_validator: "irrelevant".to_string(),
            });
        };
        pin_game_start(&mut server);

        // -- B: invalid pack id → the discard BEFORE the executor.
        client.send_encrypted(&ClientToServer::CTSPReplay {
            login_token: token,
            replay_file: replay("GOOD", "bogus_pack", pi_level),
        });
        spin(&mut server, 5);
        client.expect_no_frame();
        assert_eq!(calls.load(Ordering::SeqCst), 0, "the executor never ran");
        assert_no_score(&server);

        // -- C: invalid level id → the discard BEFORE the executor.
        client.send_encrypted(&ClientToServer::CTSPReplay {
            login_token: token,
            replay_file: replay("GOOD", cube_pack, "no_such_level"),
        });
        spin(&mut server, 5);
        client.expect_no_frame();
        assert_eq!(calls.load(Ordering::SeqCst), 0, "the executor never ran");
        assert_no_score(&server);

        // -- D: unscored level (`babysteps` — the `_RELEASE` tutorial
        // pack's `"unscored": true`; pack-qualified id) → the discard
        // BEFORE the executor.
        client.send_encrypted(&ClientToServer::CTSPReplay {
            login_token: token,
            replay_file: replay(
                "GOOD",
                tutorial_pack,
                "ohvrvanilla_vittorio_romeo_tutorial_1_babysteps",
            ),
        });
        spin(&mut server, 5);
        client.expect_no_frame();
        assert_eq!(calls.load(Ordering::SeqCst), 0, "the executor never ran");
        assert_no_score(&server);

        pin_game_start(&mut server);
        // -- E: the max-processing-time discard (the executor's
        // `std::nullopt`).
        client.send_encrypted(&ClientToServer::CTSPReplay {
            login_token: token,
            replay_file: replay("SLOW", cube_pack, pi_level),
        });
        spin(&mut server, 5);
        client.expect_no_frame();
        assert_eq!(calls.load(Ordering::SeqCst), 1, "the executor ran once");
        assert_no_score(&server);

        pin_game_start(&mut server);
        // -- F: the "difference too large" discard (the 100 s total vs the
        // ~2 s elapsed).
        client.send_encrypted(&ClientToServer::CTSPReplay {
            login_token: token,
            replay_file: replay("BADTIME", cube_pack, pi_level),
        });
        spin(&mut server, 5);
        client.expect_no_frame();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_no_score(&server);

        pin_game_start(&mut server);
        // -- G: the "bad ratio" discard (the 0.0 s total → ratio 0 < 0.65;
        // the difference ~2.0 s is < 5 — the RATIO gate fires, not the
        // difference gate).
        client.send_encrypted(&ClientToServer::CTSPReplay {
            login_token: token,
            replay_file: replay("BADRATIO", cube_pack, pi_level),
        });
        spin(&mut server, 5);
        client.expect_no_frame();
        assert_eq!(calls.load(Ordering::SeqCst), 3);
        assert_no_score(&server);

        pin_game_start(&mut server);
        // -- H: the valid path → the `addScore` (the PLAYED time is the
        // stored value — the C++'s `addScore(..., replayPlayedTime)`).
        client.send_encrypted(&ClientToServer::CTSPReplay {
            login_token: token,
            replay_file: replay("GOOD", cube_pack, pi_level),
        });
        spin(&mut server, 5);
        client.expect_no_frame();
        assert_eq!(calls.load(Ordering::SeqCst), 4);
        let ps = server
            .db()
            .get_score(stored_validator, 4242)
            .expect("score query")
            .expect("the stored score");
        assert_eq!(ps.user_name, "tester");
        assert_eq!(
            ps.score_value, 42.0,
            "the `addScore` value = `playedTimeSeconds` (the C++'s `replayPlayedTime`), not the total"
        );

        // And over the wire: the top-scores query on the (same, pack-
        // qualified) validator sees the stored row — position 0.
        client.send_encrypted(&ClientToServer::CTSPRequestTopScores {
            login_token: token,
            level_validator: stored_validator.to_string(),
        });
        spin(&mut server, 5);
        match client.read_encrypted() {
            ServerToClient::STCPTopScores { scores, .. } => {
                assert_eq!(scores.len(), 1);
                assert_eq!(scores[0].user_name, "tester");
                assert_eq!(scores[0].score_value, 42.0);
                assert_eq!(scores[0].position, 0);
            }
            other => panic!("expected STCPTopScores, got {other:?}"),
        }

        // -- I: the compressed arm (the `decompressReplayFile` success →
        // the same `processReplay`): the re-submission of the same replay
        // — the executor runs again, and the C++'s `addScore` is a no-op
        // (existing value `>=` new → no row change,
        // `Database.cpp:310-313`).
        pin_game_start(&mut server);
        let compressed = oh_replay::compress_replay_file(&replay("GOOD", cube_pack, pi_level))
            .expect("compress");
        client.send_encrypted(&ClientToServer::CTSPCompressedReplay {
            login_token: token,
            compressed_replay_file: compressed,
        });
        spin(&mut server, 5);
        client.expect_no_frame();
        assert_eq!(
            calls.load(Ordering::SeqCst),
            5,
            "the decompressed replay ran"
        );
        let ps = server
            .db()
            .get_score(stored_validator, 4242)
            .expect("score query")
            .expect("the stored score");
        assert_eq!(
            ps.score_value, 42.0,
            "the C++'s `addScore` no-op on the non-greater value"
        );
    }

    /// Construction: the binds + the supported-validators computation
    /// (the whitelist ∩ the `_RELEASE` level validators).
    #[test]
    fn construction_binds_and_computes_the_supported_validators() {
        let dir = tempfile::tempdir().expect("tempdir");
        let server = make_server(&dir);
        assert!(server.port() != 0, "the OS assigned a port");
        assert_eq!(server.client_count(), 0);
        // The whitelist is `["ohvrvanilla_vittorio_romeo_cube_1_pi_m_1"]`
        // (a default-whitelist entry — `getLevelValidator` is keyed by the
        // PACK-qualified level id, `LevelData.cpp:63-68` `getValidator`):
        // the supported set is the intersection of the whitelist with
        // `_RELEASE`'s level validators.
        assert!(
            server
                .supported_level_validators()
                .contains(&"ohvrvanilla_vittorio_romeo_cube_1_pi_m_1".to_string()),
            "the shipped `pi` level validator must survive the whitelist"
        );
    }

    /// The socket iteration + the dispatch: the accept, the
    /// `CTSPHeartbeat` arm (`true`), a state-gated `CTSPReplay` arm
    /// (`true` — no failure, the C++'s gate shape), the `PInvalid` arm,
    /// and the `CTSPDisconnect` purge.
    #[test]
    fn the_skeleton_dispatches_heartbeat_and_stubs_the_rest() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut server = make_server(&dir);

        // Connect a raw client (the accept + the state flip).
        let client_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), server.port());
        let mut stream = TcpStream::connect(client_addr).expect("connect");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("timeout");
        server.run_iteration();
        assert_eq!(server.client_count(), 1, "the client was accepted");
        assert_eq!(
            server.clients[0].state,
            ClientState::Connected,
            "the C++ sets `State::Connected` on accept"
        );

        // `CTSPHeartbeat` → the C++'s `return true` (the connection stays
        // healthy — `_lastActivity` refreshed, `_consecutiveFailures` 0).
        let mut p = Packet::new();
        crate::shared::make_client_to_server_packet(&mut p, &ClientToServer::CTSPHeartbeat);
        send_frame(&mut stream, &p);
        server.run_iteration();
        assert_eq!(
            server.clients[0].consecutive_failures, 0,
            "heartbeat: success resets the counter"
        );
        assert!(!server.clients[0].must_disconnect);

        // A state-gated arm (`CTSPReplay` in the `Connected` state, not
        // `LoggedIn_Ready`) → the C++ gate shape: `true` (NO failure,
        // the consecutive-failure counter stays 0).
        let mut p = Packet::new();
        crate::shared::make_client_to_server_packet(
            &mut p,
            &ClientToServer::CTSPReplay {
                login_token: 0,
                replay_file: oh_replay::ReplayFile::default(),
            },
        );
        send_frame(&mut stream, &p);
        server.run_iteration();
        assert_eq!(
            server.clients[0].consecutive_failures, 0,
            "the gated arm succeeds like the C++'s `return true`"
        );

        // Garbage → the C++'s `PInvalid` arm (`false` + the decode's error
        // content in the log line).
        let mut p = Packet::new();
        p.write_u8(0xFF);
        p.write_u8(0xFF);
        send_frame(&mut stream, &p);
        server.run_iteration();
        assert_eq!(
            server.clients[0].consecutive_failures, 1,
            "PInvalid: the C++ `fail` path"
        );

        // The `CTSPDisconnect` arm: `_mustDisconnect` + `Disconnected`
        // (the C++'s verbatim side effects) — the C++'s SAME-iteration
        // purge (`runIteration_PurgeClients`, `HexagonServer.cpp:518-547`)
        // then removes the client (kick + list removal), so after one
        // `run_iteration` the connection is gone.
        let mut p = Packet::new();
        crate::shared::make_client_to_server_packet(&mut p, &ClientToServer::CTSPDisconnect);
        send_frame(&mut stream, &p);
        assert_eq!(
            server.client_count(),
            1,
            "still present before the iteration"
        );
        server.run_iteration();
        assert_eq!(
            server.client_count(),
            0,
            "the disconnect arm's _mustDisconnect tripped the same-iteration purge"
        );
    }

    /// The control socket: a `verbose true` datagram (the length-prefixed
    /// `sf::Packet` string form) flips `_verbose` (the C++'s
    /// `runIteration_Control`).
    #[test]
    fn the_control_socket_parses_the_verbose_command() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut server = make_server(&dir);

        // The datagram = the C++'s `packet >> std::string` wire form:
        // u32-BE length + raw bytes.
        let control = UdpSocket::bind("127.0.0.1:0").expect("control client");
        let target = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), server.control_port());

        let send = |msg: &[u8]| {
            let mut datagram = Vec::with_capacity(4 + msg.len());
            datagram.extend_from_slice(&(msg.len() as u32).to_be_bytes());
            datagram.extend_from_slice(msg);
            control.send_to(&datagram, target).expect("send");
        };

        // `verbose true` then `verbose false` (a short retry loop — the
        // datagrams are asynchronous: each `run_iteration` consumes at
        // most one).
        send(b"verbose true");
        send(b"verbose false");
        for _ in 0..50 {
            server.run_iteration();
        }
        assert!(!server.verbose, "`verbose true` then `verbose false`");

        send(b"verbose true");
        for _ in 0..50 {
            if server.verbose {
                break;
            }
            server.run_iteration();
        }
        assert!(server.verbose, "the `verbose true` command was applied");
    }
}
