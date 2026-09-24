//! Sends every `tracing` event on to the `log` crate.
//!
//! The Flutter and uniffi bindings hand logs to the host through a `log`
//! logger. Tracing's `log` feature forwards events written with tracing's own
//! macros while no subscriber is installed, but llama-cpp-2 passes llama.cpp's
//! lines straight to the current dispatcher, so without a subscriber they are
//! silently dropped. [`forward_tracing_to_log`] installs a subscriber that
//! forwards both kinds, so the host's `log` level decides what is shown.

use std::fmt::{self, Write};
use tracing::field::{Field, Visit};
use tracing::subscriber::Interest;
use tracing::{Event, Level, Metadata};
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};

/// The target llama-cpp-2 gives the lines coming from llama.cpp itself.
const LLAMA_CPP: &str = "llama-cpp-2";

/// Installs the forwarding subscriber. Does nothing if the host already
/// installed a tracing subscriber; that one then receives everything instead.
pub fn forward_tracing_to_log() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        let subscriber = tracing_subscriber::registry().with(ForwardToLog);
        let _ = tracing::subscriber::set_global_default(subscriber);
    });
}

struct ForwardToLog;

impl<S: tracing::Subscriber> Layer<S> for ForwardToLog {
    // Decide per event: the host can change its `log` level at any time.
    fn register_callsite(&self, _: &'static Metadata<'static>) -> Interest {
        Interest::sometimes()
    }

    fn enabled(&self, metadata: &Metadata<'_>, _: Context<'_, S>) -> bool {
        log_level(metadata) <= log::max_level()
    }

    fn on_event(&self, event: &Event<'_>, _: Context<'_, S>) {
        let metadata = event.metadata();
        let mut line = Line {
            native: metadata.target() == LLAMA_CPP,
            ..Line::default()
        };
        event.record(&mut line);
        log::logger().log(
            &log::Record::builder()
                .args(format_args!("{}{}", line.message, line.fields))
                .level(log_level(metadata))
                .target(line.module.as_deref().unwrap_or(metadata.target()))
                .module_path(metadata.module_path())
                .file(metadata.file())
                .line(metadata.line())
                .build(),
        );
    }
}

/// llama.cpp reports routine progress (model metadata, backend setup) at INFO.
/// It reaches hosts as DEBUG, so apps logging at INFO only see its warnings.
fn log_level(metadata: &Metadata<'_>) -> log::Level {
    match *metadata.level() {
        Level::ERROR => log::Level::Error,
        Level::WARN => log::Level::Warn,
        Level::INFO if metadata.target() == LLAMA_CPP => log::Level::Debug,
        Level::INFO => log::Level::Info,
        Level::DEBUG => log::Level::Debug,
        Level::TRACE => log::Level::Trace,
    }
}

/// The message followed by ` key=value` pairs, the same shape tracing's own
/// `log` output has. On llama.cpp lines, the `module` field llama-cpp-2 adds
/// (such as `ggml::ggml_opencl`) becomes the log target instead.
#[derive(Default)]
struct Line {
    native: bool,
    message: String,
    fields: String,
    module: Option<String>,
}

impl Visit for Line {
    fn record_str(&mut self, field: &Field, value: &str) {
        match field.name() {
            "message" => self.message.push_str(value),
            "module" if self.native => self.module = Some(value.to_owned()),
            _ => self.record_debug(field, &value),
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if field.name() == "message" {
            let _ = write!(self.message, "{value:?}");
        } else {
            let _ = write!(self.fields, " {}={value:?}", field.name());
        }
    }
}
