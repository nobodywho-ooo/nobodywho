use godot::init::{ExtensionLibrary, InitStage};
use godot::prelude::*;
use tracing_subscriber::filter::{LevelFilter, Targets};
use tracing_subscriber::prelude::*;

mod chat;
mod convert;
mod model;
mod sampler;
mod speech_to_text;
mod task;
mod text_to_speech;
mod tools;

// --- Logging -----------------------------------------------------------------

/// Routes tracing lines to the Godot console.
struct GodotWriter;

impl std::io::Write for GodotWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if let Ok(s) = std::str::from_utf8(buf) {
            let trimmed = s.trim();
            if !trimmed.is_empty() {
                if trimmed.contains("ERROR") {
                    godot_error!("{}", trimmed);
                } else if trimmed.contains("WARN") {
                    godot_warn!("{}", trimmed);
                } else {
                    godot_print!("{}", trimmed);
                }
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for GodotWriter {
    type Writer = Self;
    fn make_writer(&'a self) -> Self::Writer {
        GodotWriter
    }
}

static INIT: std::sync::Once = std::sync::Once::new();

static LEVEL_HANDLE: std::sync::Mutex<
    Option<tracing_subscriber::reload::Handle<Targets, tracing_subscriber::Registry>>,
> = std::sync::Mutex::new(None);

fn base_directive(level: tracing::Level) -> LevelFilter {
    match level {
        tracing::Level::TRACE => LevelFilter::TRACE,
        tracing::Level::DEBUG => LevelFilter::DEBUG,
        tracing::Level::INFO => LevelFilter::INFO,
        tracing::Level::WARN => LevelFilter::WARN,
        tracing::Level::ERROR => LevelFilter::ERROR,
    }
}

// Llama logs are noisy; raise the bar so only higher-severity ones show through.
fn llama_log_threshold(level: tracing::Level) -> LevelFilter {
    match level {
        tracing::Level::TRACE => LevelFilter::TRACE,
        tracing::Level::DEBUG => LevelFilter::INFO,
        tracing::Level::INFO => LevelFilter::WARN,
        tracing::Level::WARN => LevelFilter::WARN,
        tracing::Level::ERROR => LevelFilter::ERROR,
    }
}

pub fn set_log_level(level_str: &str) {
    let level: tracing::Level = match level_str.to_uppercase().parse() {
        Ok(level) => level,
        Err(e) => {
            godot_error!("Invalid log level '{level_str}': {e}");
            return;
        }
    };

    INIT.call_once(|| {
        // XXX: uncommented for now because this seems to cause a suspicious crash
        // nobodywho::send_llamacpp_logs_to_tracing();

        let mut targets = Targets::new().with_default(base_directive(level));
        targets = targets.with_target("llama-cpp-2", llama_log_threshold(level));

        let (filter, handle) = tracing_subscriber::reload::Layer::new(targets);
        *LEVEL_HANDLE.lock().unwrap() = Some(handle);

        let fmt_layer = tracing_subscriber::fmt::layer()
            .with_writer(GodotWriter)
            .with_ansi(false)
            .with_level(true)
            .compact();

        tracing_subscriber::registry()
            .with(filter)
            .with(fmt_layer)
            .init();
    });

    if let Some(handle) = &*LEVEL_HANDLE.lock().unwrap() {
        let mut targets = Targets::new().with_default(base_directive(level));
        targets = targets.with_target("llama-cpp-2", llama_log_threshold(level));
        let _ = handle.modify(|new_targets| *new_targets = targets);
    }
}

// --- NobodyWho namespace class ---------------------------------------------

/// Global entry point for NobodyWho utilities.
///
/// A pure namespace class: not instantiable (`no_init`), exists only to host
/// instance-free static `#[func]`s — gdext has no true free/global functions,
/// so a registered class is the only home for them. Static functions are
/// callable on the class name without an instance.
///
/// ```gdscript
/// NobodyWho.set_log_level("DEBUG")
/// ```
#[derive(GodotClass)]
#[class(no_init, base=RefCounted)]
pub struct NobodyWho {
    base: Base<RefCounted>,
}

#[godot_api]
impl NobodyWho {
    /// Sets the global NobodyWho log level. One of
    /// "TRACE", "DEBUG", "INFO", "WARN", "ERROR".
    #[func]
    fn set_log_level(level: GString) {
        set_log_level(&level.to_string());
    }
}

// --- Extension entry ---------------------------------------------------------

struct NobodyWhoExtension;

#[gdextension]
unsafe impl ExtensionLibrary for NobodyWhoExtension {
    fn on_stage_init(stage: InitStage) {
        // Tracing needs Godot loaded, so init it here, not at startup.
        if stage == InitStage::Editor {
            godot_print!("NobodyWho Godot version: {}", env!("CARGO_PKG_VERSION"));
            set_log_level("INFO");
        }
    }
}
