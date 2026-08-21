pub use log::LevelFilter;

/// Configure a sensible global [logger][`log`] for the current target.
///
/// This also captures [`tracing`] events as long as either:
/// - No tracing subscriber is registered.
/// - The `tracing/log-always` feature is enabled.
///
/// NOTE: Ideally, we'd probably want to make this use `tracing` instead, and
/// set up a `tracing_log::LogTracer` for catching `log` events. But for now,
/// this will suffice.
///
/// This should be safe to call in a constructor.
///
/// # Panics
///
/// Panics if called more than once.
pub fn init(level: LevelFilter) {
    send_llamacpp_logs_to_tracing();

    // Android logs to `adb logcat`.
    #[cfg(target_os = "android")]
    {
        let config = android_logger::Config::default()
            .with_max_level(level)
            .with_tag("nobodywho");
        android_logger::init_once(config)
    }

    // iOS/tvOS/watchOS/visionOS logs to oslog (/usr/bin/log)
    #[cfg(all(target_vendor = "apple", not(target_os = "macos")))]
    {
        oslog::OsLogger::new("nobodywho")
            .level_filter(level)
            .init()
            .expect("log initialization must only happen once")
    }

    // Web platforms log to the console.
    #[cfg(target_family = "wasm")]
    {
        console_log::init_with_level(level).expect("log initialization must only happen once")
    }

    // All other platforms log to stderr.
    #[cfg(not(any(
        target_os = "android",
        all(target_vendor = "apple", not(target_os = "macos")),
        target_family = "wasm"
    )))]
    {
        env_logger::Builder::new()
            .filter_level(level)
            .parse_default_env()
            .init()
    }
}

/// Forward `llama.cpp` logs into tracing.
pub fn send_llamacpp_logs_to_tracing() {
    llama_cpp_2::send_logs_to_tracing(llama_cpp_2::LogOptions::default().with_logs_enabled(true));
}
