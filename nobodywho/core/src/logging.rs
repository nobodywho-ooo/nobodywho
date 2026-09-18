//! Log plumbing shared by the language bindings.
//!
//! Two things routinely go missing from native logs on mobile:
//!
//! 1. llama.cpp / ggml log lines. `llama-cpp-2` hands them to `tracing` by
//!    dispatching events directly on the global dispatcher, which bypasses
//!    tracing's `log` compatibility feature (that only wraps the `tracing::*!`
//!    macros). A binding that relies on the `log` crate and never installs a
//!    tracing subscriber therefore silently drops every `llama_model_loader:`,
//!    `ggml_vulkan:` and `ggml_opencl:` line. [`forward_to_log`] installs a
//!    subscriber whose only job is to turn each event into a `log::Record`.
//!
//! 2. Whatever native code writes to stdout/stderr. Android discards an app's
//!    fd 1/2, and ggml-vulkan reports e.g. the name of a shader whose pipeline
//!    the GPU driver rejected on `std::cerr` only. [`capture_native_stdio`]
//!    redirects both fds into a pipe and re-emits each line through `tracing`.

use std::sync::Once;

/// A `tracing` layer that forwards every event to the `log` crate.
pub struct LogForwardingLayer;

fn log_level(level: &tracing::Level) -> log::Level {
    match *level {
        tracing::Level::ERROR => log::Level::Error,
        tracing::Level::WARN => log::Level::Warn,
        tracing::Level::INFO => log::Level::Info,
        tracing::Level::DEBUG => log::Level::Debug,
        tracing::Level::TRACE => log::Level::Trace,
    }
}

#[derive(Default)]
struct FieldVisitor {
    message: Option<String>,
    /// `llama-cpp-2` reports the originating llama.cpp module (for example
    /// `llama.cpp::llama_model_loader`) in a `target` field.
    target: Option<String>,
    fields: Vec<(&'static str, String)>,
}

impl FieldVisitor {
    fn record(&mut self, field: &tracing::field::Field, value: String) {
        match field.name() {
            "message" => self.message = Some(value),
            "target" => self.target = Some(value),
            name => self.fields.push((name, value)),
        }
    }
}

impl tracing::field::Visit for FieldVisitor {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        // `message` and `target` are plain text; other fields keep the
        // `{:?}` formatting that tracing's own `log` bridge uses.
        if matches!(field.name(), "message" | "target") {
            self.record(field, value.to_string());
        } else {
            self.record(field, format!("{value:?}"));
        }
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.record(field, format!("{value:?}"));
    }
}

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for LogForwardingLayer {
    fn register_callsite(
        &self,
        _metadata: &'static tracing::Metadata<'static>,
    ) -> tracing::subscriber::Interest {
        // The `log` max level can change after we are installed (the binding
        // usually installs its logger first, but not necessarily), so decide
        // per event rather than caching per callsite.
        tracing::subscriber::Interest::sometimes()
    }

    fn enabled(
        &self,
        metadata: &tracing::Metadata<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) -> bool {
        log_level(metadata.level()) <= log::max_level()
    }

    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let metadata = event.metadata();
        let level = log_level(metadata.level());
        if level > log::max_level() {
            return;
        }

        let mut visitor = FieldVisitor::default();
        event.record(&mut visitor);

        // Same shape as tracing's `log` feature produces: `message; k=v k=v`.
        let mut text = visitor.message.unwrap_or_default();
        if !visitor.fields.is_empty() {
            text.push(';');
            for (name, value) in &visitor.fields {
                text.push(' ');
                text.push_str(name);
                text.push('=');
                text.push_str(value);
            }
        }
        if text.is_empty() {
            return;
        }

        let target = visitor.target.as_deref().unwrap_or(metadata.target());
        log::logger().log(
            &log::Record::builder()
                .args(format_args!("{text}"))
                .level(level)
                .target(target)
                .module_path(metadata.module_path())
                .file(metadata.file())
                .line(metadata.line())
                .build(),
        );
    }
}

/// Route all `tracing` events, including llama.cpp/ggml native log lines, to
/// the `log` crate. Call once, after (or before) the binding installs its
/// `log` logger. Idempotent.
///
/// If the host already installed a global tracing subscriber this keeps it,
/// and llama.cpp logs flow into that subscriber instead.
pub fn forward_to_log() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        use tracing_subscriber::layer::SubscriberExt;
        let subscriber = tracing_subscriber::registry().with(LogForwardingLayer);
        let _ = tracing::subscriber::set_global_default(subscriber);
        crate::send_llamacpp_logs_to_tracing();
    });
}

/// Redirect the process's stdout and stderr into `tracing` (targets
/// `native::stdout` at INFO and `native::stderr` at WARN).
///
/// Android drops an app's fd 1/2, so anything llama.cpp/ggml prints there,
/// notably ggml-vulkan's `Compute pipeline creation failed for <shader>`, is
/// otherwise lost. The write end is non-blocking so a stalled reader can only
/// ever drop output, never block the process. Idempotent.
#[cfg(target_os = "android")]
pub fn capture_native_stdio() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        for (fd, is_stderr) in [(libc::STDOUT_FILENO, false), (libc::STDERR_FILENO, true)] {
            if let Err(err) = redirect_fd(fd, is_stderr) {
                tracing::warn!(fd, %err, "could not capture native stdio");
            }
        }
    });
}

#[cfg(target_os = "android")]
fn redirect_fd(fd: libc::c_int, is_stderr: bool) -> std::io::Result<()> {
    use std::io::{BufRead, BufReader};
    use std::os::fd::FromRawFd;

    let mut ends = [0 as libc::c_int; 2];
    // SAFETY: `ends` is a valid two-element array for pipe2 to fill.
    if unsafe { libc::pipe2(ends.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    let (read_end, write_end) = (ends[0], ends[1]);

    // SAFETY: both descriptors were just returned by pipe2 and are owned here.
    let result = unsafe {
        let flags = libc::fcntl(write_end, libc::F_GETFL);
        if flags < 0 || libc::fcntl(write_end, libc::F_SETFL, flags | libc::O_NONBLOCK) < 0 {
            Err(std::io::Error::last_os_error())
        } else if libc::dup2(write_end, fd) < 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(())
        }
    };
    // SAFETY: `fd` now refers to the pipe (or we are bailing out); the extra
    // write-end descriptor is not needed either way.
    unsafe { libc::close(write_end) };
    if let Err(err) = result {
        // SAFETY: read_end is unused and owned by us.
        unsafe { libc::close(read_end) };
        return Err(err);
    }

    // SAFETY: read_end is an open descriptor that nothing else owns.
    let reader = BufReader::new(unsafe { std::fs::File::from_raw_fd(read_end) });
    let name = if is_stderr { "stderr" } else { "stdout" };
    std::thread::Builder::new()
        .name(format!("nobodywho-native-{name}"))
        .spawn(move || {
            for line in reader.split(b'\n') {
                let Ok(line) = line else { break };
                let text = String::from_utf8_lossy(&line);
                let text = text.trim_end_matches('\r');
                if text.trim().is_empty() {
                    continue;
                }
                if is_stderr {
                    tracing::warn!(target: "native::stderr", "{text}");
                } else {
                    tracing::info!(target: "native::stdout", "{text}");
                }
            }
        })?;
    Ok(())
}
