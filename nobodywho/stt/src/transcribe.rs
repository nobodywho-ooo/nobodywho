use crate::audio::load_audio;
use std::ffi::{c_char, c_void, CStr};
use whisper_rs::{FullParams, SamplingStrategy, SegmentCallbackData};

pub struct ModuleConfig {
    pub language: Option<String>,
    pub translate: bool,
    pub initial_prompt: Option<String>,
}

impl ModuleConfig {
    pub fn from_c(language: *const c_char, translate: bool, initial_prompt: *const c_char) -> Self {
        let language = if language.is_null() {
            None
        } else {
            unsafe { CStr::from_ptr(language) }
                .to_str()
                .ok()
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
        };
        let initial_prompt = if initial_prompt.is_null() {
            None
        } else {
            unsafe { CStr::from_ptr(initial_prompt) }
                .to_str()
                .ok()
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
        };
        ModuleConfig {
            language,
            translate,
            initial_prompt,
        }
    }
}

pub fn transcribe_streaming(
    state: &mut whisper_rs::WhisperState,
    config: &ModuleConfig,
    audio_path: &str,
    segment_cb: Option<extern "C" fn(*const c_char, usize, *mut c_void)>,
    done_cb: Option<extern "C" fn(*const c_char, usize, *mut c_void)>,
    userdata: *mut c_void,
) -> Result<(), String> {
    let samples = load_audio(audio_path, 16000)?;

    let mut params = build_whisper_params(config);

    if let Some(cb) = segment_cb {
        // userdata must outlive this call — caller guarantees it
        let ud = userdata;
        params.set_segment_callback_safe_lossy(move |data: SegmentCallbackData| {
            let text = data.text;
            cb(text.as_ptr() as *const c_char, text.len(), ud);
        });
    }

    state
        .full(params, &samples)
        .map_err(|e| format!("Transcription failed: {}", e))?;

    if let Some(cb) = done_cb {
        let transcript = collect_transcript(state);
        cb(
            transcript.as_ptr() as *const c_char,
            transcript.len(),
            userdata,
        );
    }

    Ok(())
}

fn build_whisper_params(config: &ModuleConfig) -> FullParams<'_, '_> {
    let n_threads = std::thread::available_parallelism()
        .map(|p| (p.get() as i32).min(8))
        .unwrap_or(4);
    let mut params = FullParams::new(SamplingStrategy::BeamSearch {
        beam_size: 5,
        patience: -1.0,
    });
    params.set_n_threads(n_threads);
    params.set_translate(config.translate);
    params.set_language(config.language.as_deref());
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_special(false);
    if let Some(ref prompt) = config.initial_prompt {
        params.set_initial_prompt(prompt);
    }
    params
}

fn collect_transcript(state: &whisper_rs::WhisperState) -> String {
    (0..state.full_n_segments())
        .filter_map(|i| state.get_segment(i))
        .map(|seg| seg.to_string())
        .collect::<String>()
        .trim()
        .to_string()
}
