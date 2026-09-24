//! llama.cpp's own log lines reach the `log` crate once
//! `forward_tracing_to_log` is installed. They go through the real llama.cpp
//! log callback, the path the Flutter and uniffi bindings rely on. This is its
//! own test binary because it installs a global `log` logger.

use std::ffi::{c_char, c_void, CStr};
use std::ptr::null_mut;
use std::sync::Mutex;

static RECORDS: Mutex<Vec<(log::Level, String, String)>> = Mutex::new(Vec::new());

struct Capture;

impl log::Log for Capture {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        let entry = (
            record.level(),
            record.target().to_owned(),
            record.args().to_string(),
        );
        RECORDS.lock().unwrap().push(entry);
    }

    fn flush(&self) {}
}

type LogCallback = Option<unsafe extern "C" fn(i32, *const c_char, *mut c_void)>;

extern "C" {
    fn llama_log_get(callback: *mut LogCallback, user_data: *mut *mut c_void);
}

const GGML_LOG_LEVEL_INFO: i32 = 2;
const GGML_LOG_LEVEL_WARN: i32 = 3;

/// Logs `text` the way llama.cpp does: through the callback llama-cpp-2 set.
fn llama_cpp_log(level: i32, text: &CStr) {
    let mut callback: LogCallback = None;
    let mut user_data = null_mut();
    unsafe { llama_log_get(&mut callback, &mut user_data) };
    let callback = callback.expect("llama-cpp-2 installed a log callback");
    unsafe { callback(level, text.as_ptr(), user_data) };
}

#[test]
fn llama_cpp_lines_reach_log() {
    log::set_logger(&Capture).unwrap();
    log::set_max_level(log::LevelFilter::Trace);
    nobodywho::send_llamacpp_logs_to_tracing();
    nobodywho::logging::forward_tracing_to_log();

    llama_cpp_log(
        GGML_LOG_LEVEL_INFO,
        c"llama_model_loader: loaded meta data\n",
    );
    llama_cpp_log(GGML_LOG_LEVEL_WARN, c"llama_model_loader: odd tensor\n");
    tracing::info!(answer = 42, "from nobodywho");

    let target = "ggml::llama_model_loader".to_owned();
    assert_eq!(
        *RECORDS.lock().unwrap(),
        vec![
            (log::Level::Debug, target.clone(), "loaded meta data".into()),
            (log::Level::Warn, target, "odd tensor".into()),
            (
                log::Level::Info,
                "log_forwarding".into(),
                "from nobodywho answer=42".into()
            ),
        ]
    );
}
