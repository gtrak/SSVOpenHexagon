//! The engine-log sink (lane 026 W5: the `[channel]` lines must reach the
//! browser console on web).
//!
//! The C++ prints the `ssvu::lo(channel)` lines to stdout; the port's
//! default sink does the same (`println!("[{channel}] {message}")`). On
//! `wasm32-unknown-emscripten` the Rust stdio does not reach the browser
//! console in the current emscripten glue (the fd 1/2 writes are swallowed
//! by the default `/dev/stdout -> /dev/tty` null path), so `oh-web`
//! installs a browser-console sink at startup (before `main` runs) via
//! [`set_log_sink`]; every other platform keeps the stdout default.

use std::sync::Mutex;

static SINK: Mutex<Option<Box<dyn Fn(&str, &str) + Send + Sync>>> = Mutex::new(None);

/// Install the sink that receives engine log lines (lane 026: the web
/// browser-console sink). The pair is the raw `(channel, message)`.
pub fn set_log_sink(f: impl Fn(&str, &str) + Send + Sync + 'static) {
    *SINK.lock().expect("log sink poisoned") = Some(Box::new(f));
}

/// Emit one engine log line. With no sink installed the line goes to
/// stdout, channel-prefixed (the C++ parity): `[{channel}] {message}`.
pub fn emit_log(channel: &str, message: &str) {
    if let Ok(guard) = SINK.lock() {
        if let Some(f) = guard.as_ref() {
            f(channel, message);
            return;
        }
    }
    println!("[{channel}] {message}");
}
