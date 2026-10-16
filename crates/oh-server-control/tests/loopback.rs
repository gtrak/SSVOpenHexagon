//! Issue 19: `oh-server-control` pinned against a REAL UDP loopback listener
//! (the exact command bytes on the wire) + subprocess runs of the REAL
//! binary (the exact CLI parsing + output text).
//!
//! The control protocol (D-014, verified against source): one UDP datagram to
//! 127.0.0.1:50506 (main.cpp:39-40) carrying an `sf::Packet` string —
//! u32-BE byte count + raw bytes (main.cpp:37; fork `Packet.cpp:521`).

use oh_server_control::ControlSender as _;
use std::io::Write as _;
use std::net::UdpSocket;
use std::process::{Command, Stdio};
use std::sync::{Mutex, MutexGuard};

/// The two tests that touch the REAL control port (50506) must not run
/// concurrently: the one-off test's datagram would land in the other
/// test's listener (or vice versa).
static CONTROL_PORT_LOCK: Mutex<()> = Mutex::new(());

fn lock_control_port() -> MutexGuard<'static, ()> {
    CONTROL_PORT_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// The exact wire bytes of one control datagram: u32-BE length + raw bytes.
fn datagram(message: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + message.len());
    out.extend_from_slice(&(message.len() as u32).to_be_bytes());
    out.extend_from_slice(message);
    out
}

#[test]
fn loopback_asserts_the_exact_command_bytes() {
    // A free loopback port (the binary itself hardcodes 50506 — 1:1 with the
    // C++; the lib-level real-socket send is exercised here on the free
    // port instead).
    let listener = UdpSocket::bind("127.0.0.1:0").expect("bind loopback");
    let port = listener.local_addr().expect("local addr").port();

    let mut sender = oh_server_control::UdpControlSender::new();
    assert!(sender.send(port, &oh_server_control::encode_control_packet(b"verbose")));

    listener
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .expect("read timeout");
    let mut buf = [0u8; 512];
    let (n, _from) = listener.recv_from(&mut buf).expect("recv control datagram");
    // The server's decode (`packet >> controlMsg`, HexagonServer.cpp:334) must
    // see exactly the C++ bytes: u32-BE length + the command text.
    assert_eq!(&buf[..n], datagram(b"verbose"));
}

#[test]
fn binary_one_off_is_silent_and_exits_0() {
    // No listener on 50506 is needed: a UDP send to a closed loopback port
    // succeeds on Linux/macOS (no synchronous error), so this exercises the
    // binary's one-off path end to end.
    let _port_lock = lock_control_port();
    let out = Command::new(env!("CARGO_BIN_EXE_oh-server-control"))
        .arg("verbose")
        .stdin(Stdio::null())
        .output()
        .expect("run binary");
    assert_eq!(
        out.status.code(),
        Some(0),
        "stdout: {:?} stderr: {:?}",
        out.stdout,
        out.stderr
    );
    assert!(out.stdout.is_empty(), "stdout: {:?}", out.stdout);
    assert!(out.stderr.is_empty(), "stderr: {:?}", out.stderr);
}

#[test]
fn binary_rejects_more_than_one_argument() {
    let out = Command::new(env!("CARGO_BIN_EXE_oh-server-control"))
        .args(["a", "b"])
        .stdin(Stdio::null())
        .output()
        .expect("run binary");
    // C++ `return -1` → low byte 255 (main.cpp:24-28).
    assert_eq!(out.status.code(), Some(255), "stderr: {:?}", out.stderr);
    assert!(out.stdout.is_empty());
    assert_eq!(out.stderr, b"Invalid number of arguments\n");
}

#[test]
fn binary_interactive_exits_1_with_the_exact_error_line_at_eof() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_oh-server-control"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn binary");
    {
        // Drop-only: closing stdin delivers the EOF.
        let _stdin = child.stdin.take().expect("stdin");
    }
    let out = child.wait_with_output().expect("wait");
    // Documented deviation (D-014): the C++ spins on this line forever at
    // EOF (main.cpp:51-60); the port prints it once and exits 1.
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    assert_eq!(out.stderr, b"Error reading line from stdin\n");
}

/// The full interactive path against the REAL hardcoded port 50506 — skipped
/// (with a printed reason) if something already binds it on this machine.
#[test]
fn binary_interactive_sends_the_exact_datagram_to_50506() {
    let _port_lock = lock_control_port();
    let Ok(listener) = UdpSocket::bind("127.0.0.1:50506") else {
        eprintln!("test skipped: 127.0.0.1:50506 is already in use");
        return;
    };
    let _ = listener.set_read_timeout(Some(std::time::Duration::from_secs(10)));

    let mut child = Command::new(env!("CARGO_BIN_EXE_oh-server-control"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn binary");
    {
        let mut stdin = child.stdin.take().expect("stdin");
        stdin.write_all(b"verbose true\n").expect("write command");
        stdin.flush().expect("flush");
    } // drop → EOF → the binary sends the line, then prints the EOF error and exits 1
    let out = child.wait_with_output().expect("wait");
    assert_eq!(out.status.code(), Some(1), "stderr: {:?}", out.stderr);
    assert_eq!(out.stderr, b"Error reading line from stdin\n");

    let mut buf = [0u8; 512];
    let (n, from) = listener.recv_from(&mut buf).expect("recv control datagram");
    assert_eq!(
        from.ip(),
        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)
    );
    // The exact C++ bytes (main.cpp:37: `packet << stringBuf`).
    assert_eq!(&buf[..n], datagram(b"verbose true"));
}
