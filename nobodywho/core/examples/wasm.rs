use emscripten_functions::emscripten::set_main_loop_with_arg;
use nobodywho::chat::{Chat, ChatConfig, TurnGenerator};
use nobodywho::llm;
use std::ops::ControlFlow;
use std::sync::Arc;
use tracing::Event;
use tracing_subscriber::fmt::format::{DefaultFields, Writer};
use tracing_subscriber::fmt::{FmtContext, FormatFields};
use tracing_subscriber::Registry;

fn main() {
    nobodywho::send_llamacpp_logs_to_tracing();

    let f: fn(
        &FmtContext<'_, Registry, DefaultFields>,
        Writer<'_>,
        &Event<'_>,
    ) -> core::fmt::Result = |ctx, mut w, e| {
        let meta = e.metadata();
        write!(
            w,
            "{:>5} {}{}: ",
            meta.level(),
            meta.target(),
            if meta.is_span() {
                format!("::{}", meta.name())
            } else {
                "".to_string()
            }
        )?;
        ctx.field_format().format_fields(w.by_ref(), e)?;
        writeln!(w)
    };

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
        .event_format(f)
        .init();

    // tracing_wasm::try_set_as_global_default().ok();

    tracing::trace!("test");

    tracing::debug!("test");

    tracing::info!("test");

    let mut args = std::env::args();
    let _ = args.next().unwrap();
    let path = args.next().expect("model path");
    let model = Arc::new(
        llm::get_model(&path, false, None, None, None)
            .unwrap_or_else(|e| panic!("failed to load test model from {path:?}: {e}")),
    );
    tracing::info!("LOADED");

    let chat = Chat::new(
        model,
        ChatConfig {
            n_ctx: 1024,
            ..Default::default()
        },
    )
    .unwrap();
    tracing::info!("CHAT CREATED");

    set_main_loop_with_arg(
        |(chat, state)| {
            if let Some(generator) = state {
                match generator.next(chat, None, true) {
                    Ok(ControlFlow::Continue(text)) => {
                        for text in text {
                            tracing::info!("token: {text:?}");
                        }
                    }
                    Ok(ControlFlow::Break(response)) => {
                        println!();
                        println!("{}", response.content);
                        println!();
                        *state = None;
                    }
                    Err(e) => {
                        tracing::error!(%e, "error");
                        std::process::exit(0);
                    }
                }
            } else {
                let mut prompt = String::new();
                match std::io::stdin().read_line(&mut prompt) {
                    Ok(_) => {
                        let generator = chat.ask(prompt).unwrap();
                        *state = Some(generator);
                    }
                    Err(e) => tracing::error!("{e}"),
                }
            }
        },
        (chat, None::<TurnGenerator>),
        0,
        false,
    );

    tracing::info!("DONE");
}
