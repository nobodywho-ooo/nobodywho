//! Forward native diagnostics to the binding's logger.
use std::sync::Once;
use tracing_subscriber::layer::SubscriberExt;

struct LogForwardingLayer;

#[derive(Default)]
struct Fields {
    message: String,
    target: Option<String>,
    extra: String,
}

impl tracing::field::Visit for Fields {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        match field.name() {
            "message" => self.message = value.into(),
            "target" => self.target = Some(value.into()),
            _ => self.record_debug(field, &value),
        }
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        use std::fmt::Write;
        match field.name() {
            "message" => self.message = format!("{value:?}"),
            "target" => self.target = Some(format!("{value:?}")),
            name => {
                let _ = write!(self.extra, " {name}={value:?}");
            }
        }
    }
}

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for LogForwardingLayer {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        let metadata = event.metadata();
        let level = match *metadata.level() {
            tracing::Level::ERROR => log::Level::Error,
            tracing::Level::WARN => log::Level::Warn,
            tracing::Level::INFO => log::Level::Info,
            tracing::Level::DEBUG => log::Level::Debug,
            tracing::Level::TRACE => log::Level::Trace,
        };
        if level > log::max_level() {
            return;
        }
        let mut fields = Fields::default();
        event.record(&mut fields);
        log::log!(target: fields.target.as_deref().unwrap_or(metadata.target()), level,
            "{}{}", fields.message, fields.extra);
    }
}

/// llama-cpp-2 dispatches events directly, bypassing tracing's `log` feature.
pub fn forward_to_log() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        let _ = tracing::subscriber::set_global_default(
            tracing_subscriber::registry().with(LogForwardingLayer),
        );
        crate::send_llamacpp_logs_to_tracing();
    });
}

pub fn enable_native_traces() {
    #[cfg(target_os = "android")]
    {
        static INSTALL: Once = Once::new();
        INSTALL.call_once(|| {
            if std::env::var_os("OCL_ICD_ENABLE_TRACE").is_none() {
                std::env::set_var("OCL_ICD_ENABLE_TRACE", "1");
            }
        });
    }
}

/// Android discards stdout/stderr; capture driver diagnostics as well as GGML logs.
#[cfg(target_os = "android")]
pub fn capture_native_stdio() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        for fd in [libc::STDOUT_FILENO, libc::STDERR_FILENO] {
            if let Err(err) = redirect_fd(fd) {
                tracing::warn!(fd, %err, "could not capture native stdio");
            }
        }
    });
}

#[cfg(target_os = "android")]
fn redirect_fd(fd: libc::c_int) -> std::io::Result<()> {
    use std::io::{BufRead, BufReader};
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    let mut ends = [0; 2];
    // SAFETY: the array has space for both descriptors; OwnedFd closes each once.
    let (reader, writer) = unsafe {
        if libc::pipe2(ends.as_mut_ptr(), libc::O_CLOEXEC) != 0 {
            return Err(std::io::Error::last_os_error());
        }
        (OwnedFd::from_raw_fd(ends[0]), OwnedFd::from_raw_fd(ends[1]))
    };
    // Only writes are non-blocking: diagnostics must never stall inference.
    // SAFETY: writer is owned here; dup2 deliberately replaces stdout/stderr.
    unsafe {
        if libc::fcntl(writer.as_raw_fd(), libc::F_SETFL, libc::O_NONBLOCK) < 0
            || libc::dup2(writer.as_raw_fd(), fd) < 0
        {
            return Err(std::io::Error::last_os_error());
        }
    }
    std::thread::Builder::new()
        .name(format!("native-log-{fd}"))
        .spawn(move || {
            for line in BufReader::new(std::fs::File::from(reader)).split(b'\n') {
                let Ok(line) = line else { break };
                let text = String::from_utf8_lossy(&line);
                if fd == libc::STDERR_FILENO {
                    tracing::warn!(target: "native::stderr", "{}", text.trim_end());
                } else {
                    tracing::info!(target: "native::stdout", "{}", text.trim_end());
                }
            }
        })?;
    Ok(())
}
