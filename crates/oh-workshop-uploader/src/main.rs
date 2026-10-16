//! `oh-workshop-uploader` — the binary wrapper (issue 19).
//!
//! The C++ `main` (main.cpp:711) takes no arguments at all (the tool is a
//! purely interactive menu; argv is ignored). The default
//! [`oh_workshop_uploader::steam::SteamUgc`] impl is `NoSteam` (the
//! `TODO(issue-20)` no-SDK seam), so without the Steam runtime the binary
//! prints the C++'s init-failure lines and exits 1 — exactly the C++'s
//! behavior on a machine without Steam (main.cpp:716-719).

fn main() -> std::process::ExitCode {
    let mut input = std::io::stdin().lock();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let mut steam = oh_workshop_uploader::steam::NoSteam;
    let code = oh_workshop_uploader::run(&mut steam, &mut input, &mut out);

    // The C++ `main` return values are 0 / 1 (main.cpp:711-737). The low-byte
    // reduction is the same convention as `oh-app`'s main; returning from
    // `main` (rather than `std::process::exit`) keeps the runtime's stdout
    // flush on the way out.
    std::process::ExitCode::from((code & 0xff) as u8)
}
