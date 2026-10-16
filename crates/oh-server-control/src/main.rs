//! `oh-server-control` — the binary wrapper (issue 19).
//!
//! Wires [`oh_server_control::run`] to the real sources: argv (collected as
//! raw byte buffers — the C++ `argv[1]` is a byte buffer, main.cpp:65),
//! stdin, stderr, and a real UDP socket.
//!
//! The exit code is the C++ `main` return value reduced to its low byte —
//! POSIX status semantics, so the C++ `return -1` becomes 255 exactly as in
//! C++ (same convention as `oh-app`'s main).

fn main() -> std::process::ExitCode {
    // The C++ `argv` elements are byte buffers (main.cpp:65:
    // `stringBuf = argv[1]`); collect the platform bytes verbatim (on
    // Linux/macOS the byte stream is exact; on Windows non-UTF-8 strings
    // are lossily encoded by `as_encoded_bytes` — Windows parity is issue 22).
    let args: Vec<Vec<u8>> = std::env::args_os()
        .map(|a| a.as_encoded_bytes().to_vec())
        .collect();

    let code = {
        let mut input = std::io::stdin().lock();
        let mut stderr = std::io::stderr().lock();
        let mut sender = oh_server_control::UdpControlSender::new();
        oh_server_control::run(&args, &mut input, &mut stderr, &mut sender)
    };

    // The C++ return values are -1 / 0 / 1; the low byte is the POSIX exit
    // status (C++ `return -1` → 255). Returning from `main` (rather than
    // `std::process::exit`) keeps the runtime's stderr flush on the way out.
    std::process::ExitCode::from((code & 0xff) as u8)
}
