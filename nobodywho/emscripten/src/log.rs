//! Rust `log` records are routed to the JS `console` method matching their
//! level (`console.debug`, `console.info`, `console.warn`, `console.error`).
//! Plain stderr logging would put everything on `console.error`, which is both
//! noisy and hides the level.
//!
//! This uses `emscripten_log` with `EM_LOG_CONSOLE`, which is built into the
//! emscripten runtime — see `<emscripten/console.h>` and
//! `_emscripten_log_formatted` in emscripten's `libcore.js`. So we can declare
//! and call it directly, without pulling in `web-sys`/`wasm-bindgen`.use std::ffi::CString;
use std::ffi::{CString, c_char, c_int};

/// If specified, logs directly to the browser console/inspector window.
/// If not specified, logs via the application Module.
const EM_LOG_CONSOLE: c_int = 1;

/// If specified, prints a warning message (combined with `EM_LOG_CONSOLE`).
const EM_LOG_WARN: c_int = 2;

/// If specified, prints an error message (combined with `EM_LOG_CONSOLE`).
///
/// If neither `EM_LOG_WARN`, `EM_LOG_ERROR`, `EM_LOG_INFO` nor
/// `EM_LOG_DEBUG` is specified, a log message is printed. `EM_LOG_WARN`,
/// `EM_LOG_INFO`, `EM_LOG_DEBUG` and `EM_LOG_ERROR` are mutually exclusive.
///
/// If `EM_LOG_CONSOLE` is not specified then the message will be outputted
/// via err() (for `EM_LOG_ERROR` or `EM_LOG_WARN`) or out() otherwise.
const EM_LOG_ERROR: c_int = 4;

/// If specified, prints a call stack that contains file names referring to
/// original C sources using source map information.
#[allow(dead_code)]
const EM_LOG_C_STACK: c_int = 8;

/// If specified, the pathnames of the file information in the call stack will be omitted.
#[allow(dead_code)]
const EM_LOG_NO_PATHS: c_int = 32;

/// If specified, prints a debug message to console (combined with `EM_LOG_CONSOLE`).
const EM_LOG_DEBUG: c_int = 256;

/// If specified, prints an info message to console (combined with `EM_LOG_CONSOLE`).
const EM_LOG_INFO: c_int = 512;

// SAFETY: The signature is correct.
// TODO: Unsure about unwinding, but it probably can't unwind.
unsafe extern "C" {
    /// Prints out a message to the console, optionally with the callstack information.
    ///
    /// Parameters:
    /// flags: A binary OR of items from the list of `EM_LOG_xxx <EM_LOG_CONSOLE>` flags that specify printing options.
    /// format: A ``printf``-style format string.
    /// ...: A ``printf``-style "..." parameter list that is parsed according to the ``printf`` formatting rules.
    unsafe fn emscripten_log(flags: c_int, format: *const c_char, ...);
}

/// Initialize a global `log` logger that writes to the JS console.
///
/// Must only be called once per process; panics otherwise. This also captures
/// `tracing` events when `tracing` has no subscriber installed and its `log`
/// feature is enabled.
pub fn init(level: log::LevelFilter) {
    log::set_boxed_logger(Box::new(ConsoleLogger { level }))
        .expect("log initialization must only happen once");
    log::set_max_level(level);
}

/// A `log::Log` implementation that writes to the JS `console` method matching
/// the record's level.
struct ConsoleLogger {
    level: log::LevelFilter,
}

impl ConsoleLogger {
    fn emit(&self, message: &str, flags: c_int) {
        let c_message = CString::new(message.replace('\0', "[NUL]"))
            .expect("message without NUL bytes is always valid CString");
        // SAFETY:
        // - The format string is a valid `printf`-style C-string, which
        //   specifies one C-string argument.
        // - The vararg is a valid C-string, as required by the format.
        // - Passing incorrect flags doesn't cause unsoundness, so we don't
        //   have to consider that.
        // - The function just logs, so it should be thread-safe.
        unsafe { emscripten_log(flags, c"%s".as_ptr(), c_message.as_ptr()) }
    }
}

impl log::Log for ConsoleLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= self.level
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let message = format!("[{}] {}", record.target(), record.args());

        match record.level() {
            log::Level::Error => self.emit(&message, EM_LOG_CONSOLE | EM_LOG_ERROR),
            log::Level::Warn => self.emit(&message, EM_LOG_CONSOLE | EM_LOG_WARN),
            log::Level::Info => self.emit(&message, EM_LOG_CONSOLE | EM_LOG_INFO),
            log::Level::Debug => self.emit(&message, EM_LOG_CONSOLE), // console.log
            log::Level::Trace => self.emit(&message, EM_LOG_CONSOLE | EM_LOG_DEBUG),
        }
    }

    fn flush(&self) {}
}
