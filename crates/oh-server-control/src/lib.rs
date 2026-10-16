//! Port of `src/OHServerControl/main.cpp` (68 lines) — the server-control CLI
//! tool.
//!
//! # The control protocol (D-014: verified against source)
//!
//! The tool sends ONE UDP datagram to `sf::IpAddress::LocalHost` (127.0.0.1)
//! port `50506` (main.cpp:39-40). The datagram is an `sf::Packet` containing
//! exactly one string — `packet << stringBuf` (main.cpp:37) — in the fork's
//! `std::string` form: **u32-BE byte count + raw bytes** (SFML fork
//! `src/SFML/Network/Packet.cpp:521`, pinned in
//! `.agents/skills/lib-sfml-packet.md` and re-used here through
//! `oh_net::packet::Packet`, the codec oh-net already pins).
//!
//! This is NOT the `oh`-preamble game protocol in `oh_net::shared` (that is
//! the TCP client/server wire). The control port is a separate, fire-and-
//! forget, string-form UDP channel: the tool never receives anything.
//!
//! The receive side (cited for completeness — the tool only ever sends):
//! `HexagonServer::initializeControlSocket` binds a blocking `sf::UdpSocket`
//! to the config's `server_control_port` (default 50506,
//! `Global/Config.cpp:352`) on LocalHost
//! (`include/SSVOpenHexagon/Core/HexagonServer.hpp:48-50`,
//! `Core/HexagonServer.cpp:82-96`); `runIteration_Control` receives the
//! datagram, decodes the string via `packet >> controlMsg`
//! (`HexagonServer.cpp:326-337`), splits it on whitespace
//! (`HexagonServer.cpp:347-352`), and dispatches the command
//! (`HexagonServer.cpp:354-368`: `verbose true|false`; the `db exec` arm is
//! `#if 0`-ed out).
//!
//! # Documented deviations (D-014)
//!
//! - EOF in interactive mode: once `eofbit` is set, C++ `std::getline` keeps
//!   failing, so the C++ spins forever printing
//!   `Error reading line from stdin` (main.cpp:51-60). The port prints that
//!   line once and exits with status 1.
//! - The `argc < 1` arm (main.cpp:18-22) is unreachable even in C++ (`argc`
//!   is always >= 1; the Rust `args_os` iterator always yields the program
//!   name), so its text is not emitted here.
//!
//! # Testability
//!
//! [`run`] takes the line reader, the stderr sink, and the datagram sender
//! as parameters — the binary wires those to stdin/stderr and a real UDP
//! socket, with the port hardcoded to 50506 exactly as the C++
//! (main.cpp:39-40).

use std::io::{BufRead, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};

/// C++ `sf::IpAddress::LocalHost` (main.cpp:39).
pub const CONTROL_HOST: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);
/// The port the C++ hardcodes (main.cpp:39-40). The server's own default is
/// the same value via config `server_control_port` (`Global/Config.cpp:352`).
pub const CONTROL_PORT: u16 = 50506;

/// C++ `controlSocket.send(packet, sf::IpAddress::LocalHost, 50506)`
/// (main.cpp:39-40) — one datagram, fire and forget.
pub trait ControlSender {
    fn send(&mut self, port: u16, datagram: &[u8]) -> bool;
}

/// The real sender: one std `UdpSocket` for the whole run — the C++'s single
/// `controlSocket` local (main.cpp:32).
pub struct UdpControlSender {
    socket: UdpSocket,
}

impl UdpControlSender {
    /// One std `UdpSocket` bound to an ephemeral loopback port — the C++
    /// `sf::UdpSocket` (main.cpp:32) is likewise a fresh socket that only
    /// sends (its first `send` auto-binds in SFML).
    pub fn new() -> Self {
        Self {
            socket: UdpSocket::bind((CONTROL_HOST, 0)).expect("udp socket"),
        }
    }
}

impl Default for UdpControlSender {
    fn default() -> Self {
        Self::new()
    }
}

impl ControlSender for UdpControlSender {
    fn send(&mut self, port: u16, datagram: &[u8]) -> bool {
        self.socket
            .send_to(datagram, SocketAddr::new(CONTROL_HOST, port))
            .is_ok()
    }
}

/// C++ `packet << stringBuf` (main.cpp:37): the SFML `std::string` wire form
/// — u32-BE byte count + raw bytes (fork `Packet.cpp:521`). Reuses the codec
/// oh-net pins (`oh_net::packet::Packet`).
pub fn encode_control_packet(message: &[u8]) -> Vec<u8> {
    let mut p = oh_net::packet::Packet::new();
    // `write_str` writes exactly u32-BE length + raw bytes; the
    // `from_utf8_unchecked` is sound — the writer only reads the byte length
    // and the bytes themselves (the C++ `stringBuf` is a raw byte buffer).
    p.write_str(unsafe { std::str::from_utf8_unchecked(message) });
    p.data().to_vec()
}

/// C++ `cin_getline_string` (main.cpp:9-13): one `std::getline` — raw bytes
/// up to (and consuming) a single trailing `\n`; a lone `\r` is kept (the
/// C++ strips only `\n`). `None` at EOF or on a read error — including a
/// final line WITHOUT a newline: the C++ sets eofbit and its caller uses only
/// the `bool`, so the partial data is discarded (same here).
fn read_line(input: &mut dyn BufRead) -> Option<Vec<u8>> {
    let mut buf = Vec::new();
    match input.read_until(b'\n', &mut buf) {
        Ok(0) => None,
        Ok(_) if buf.last() == Some(&b'\n') => {
            buf.pop();
            Some(buf)
        }
        _ => None,
    }
}

/// Port of the C++ `main` dispatch (main.cpp:16-68). `args` are the argv byte
/// buffers (the binary collects `std::env::args_os`; `args[0]` is the program
/// name, so the C++ `argc` checks are `args.len()` checks).
///
/// Returns the C++ `main` return value (-1 / 0 / 1 — the binary reduces it to
/// its low byte, matching `oh-app`'s convention).
pub fn run(
    args: &[Vec<u8>],
    input: &mut dyn BufRead,
    stderr: &mut dyn Write,
    sender: &mut dyn ControlSender,
) -> i32 {
    // main.cpp:18-22 (`argc < 1`) is unreachable even in C++.
    // main.cpp:24-28.
    if args.len() > 2 {
        let _ = writeln!(stderr, "Invalid number of arguments");
        return -1;
    }

    // main.cpp:34-47: the `sendToServer` lambda (a fresh `packet` per call).
    let mut send_to_server = |string_buf: &[u8]| -> bool {
        let datagram = encode_control_packet(string_buf);
        if !sender.send(CONTROL_PORT, &datagram) {
            let _ = writeln!(stderr, "Error sending control packet");
            return false;
        }
        true
    };

    if args.len() == 1 {
        // main.cpp:49-61: interactive mode.
        loop {
            match read_line(input) {
                Some(line) => {
                    send_to_server(&line); // the C++ ignores the send result here
                }
                None => {
                    // Documented deviation: the C++ spins on this line forever
                    // once stdin hits EOF (main.cpp:51-60); the port prints it
                    // once and exits 1.
                    let _ = writeln!(stderr, "Error reading line from stdin");
                    return 1;
                }
            }
        }
    }

    // main.cpp:63-67: one-off send.
    if send_to_server(&args[1]) {
        0
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A capturing fake sender.
    struct FakeSender {
        sent: Vec<Vec<u8>>,
        fail_from: usize, // fail every send with index >= fail_from
    }

    impl FakeSender {
        fn new() -> Self {
            Self {
                sent: Vec::new(),
                fail_from: usize::MAX,
            }
        }
        fn failing() -> Self {
            Self {
                sent: Vec::new(),
                fail_from: 0,
            }
        }
    }

    impl ControlSender for FakeSender {
        fn send(&mut self, _port: u16, datagram: &[u8]) -> bool {
            if self.sent.len() >= self.fail_from {
                return false;
            }
            self.sent.push(datagram.to_vec());
            true
        }
    }

    /// The C++ `sf::Packet` string form: u32-BE length + raw bytes.
    #[test]
    fn encode_is_u32be_length_plus_raw_bytes() {
        assert_eq!(
            encode_control_packet(b"verbose"),
            [0, 0, 0, 7, b'v', b'e', b'r', b'b', b'o', b's', b'e']
        );
        // Empty line (C++ sends the 4-byte zero length, the server treats an
        // empty controlMsg as a no-op, HexagonServer.cpp:344-346).
        assert_eq!(encode_control_packet(b""), [0, 0, 0, 0]);
        // Multi-byte UTF-8: the count is BYTES (C++ `std::string::getSize`).
        assert_eq!(
            encode_control_packet("é".as_bytes()),
            [0, 0, 0, 2, 0xC3, 0xA9]
        );
    }

    #[test]
    fn one_off_success_sends_exactly_one_datagram() {
        let mut sender = FakeSender::new();
        let mut input: &[u8] = b"";
        let mut stderr: Vec<u8> = Vec::new();
        let args = vec![b"oh-server-control".to_vec(), b"hello".to_vec()];
        let code = run(&args, &mut input, &mut stderr, &mut sender);
        assert_eq!(code, 0);
        assert!(stderr.is_empty());
        assert_eq!(sender.sent, vec![encode_control_packet(b"hello")]);
    }

    #[test]
    fn one_off_send_failure_prints_error_and_exits_1() {
        let mut sender = FakeSender::failing();
        let mut input: &[u8] = b"";
        let mut stderr: Vec<u8> = Vec::new();
        let args = vec![b"oh-server-control".to_vec(), b"hello".to_vec()];
        let code = run(&args, &mut input, &mut stderr, &mut sender);
        assert_eq!(code, 1);
        assert_eq!(stderr, b"Error sending control packet\n");
    }

    #[test]
    fn too_many_arguments_prints_error_and_exits_minus_1() {
        let mut sender = FakeSender::new();
        let mut input: &[u8] = b"";
        let mut stderr: Vec<u8> = Vec::new();
        let args = vec![b"prog".to_vec(), b"a".to_vec(), b"b".to_vec()];
        let code = run(&args, &mut input, &mut stderr, &mut sender);
        assert_eq!(code, -1);
        assert_eq!(stderr, b"Invalid number of arguments\n");
        assert!(sender.sent.is_empty());
    }

    #[test]
    fn interactive_sends_each_line_then_exits_1_at_eof() {
        let mut sender = FakeSender::new();
        let mut input: &[u8] = b"verbose true\n\nplain\r\n";
        let mut stderr: Vec<u8> = Vec::new();
        let args = vec![b"prog".to_vec()];
        let code = run(&args, &mut input, &mut stderr, &mut sender);
        // Documented deviation: the C++ would spin at EOF; the port exits 1
        // after one error line.
        assert_eq!(code, 1);
        assert_eq!(stderr, b"Error reading line from stdin\n");
        // Byte-exact: `\r` is KEPT (the C++ getline strips only `\n`).
        assert_eq!(
            sender.sent,
            vec![
                encode_control_packet(b"verbose true"),
                encode_control_packet(b""),
                encode_control_packet(b"plain\r"),
            ]
        );
    }

    #[test]
    fn interactive_prints_send_error_and_keeps_reading() {
        let mut sender = FakeSender::new();
        sender.fail_from = 1; // the second send fails
        let mut input: &[u8] = b"a\nb\n";
        let mut stderr: Vec<u8> = Vec::new();
        let args = vec![b"prog".to_vec()];
        let code = run(&args, &mut input, &mut stderr, &mut sender);
        assert_eq!(code, 1);
        assert_eq!(
            stderr,
            b"Error sending control packet\nError reading line from stdin\n"
        );
        assert_eq!(sender.sent, vec![encode_control_packet(b"a")]);
    }
}
