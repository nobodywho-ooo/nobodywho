use std::pin::pin;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, LazyLock};

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::LlamaChatMessage;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::model::{AddBos, Special};
use llama_cpp_2::sampling::params::LlamaSamplerChainParams;
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::token::LlamaToken;
use llama_cpp_2::LlamaSamplerError;

static LLAMA_BACKEND: LazyLock<LlamaBackend> =
    LazyLock::new(|| LlamaBackend::init().expect("Failed to initialize llama backend"));

pub enum LLMOutput {
    Token(String),
    Done,
}

pub type Model = Arc<LlamaModel>;

pub fn has_discrete_gpu() -> bool {
    // Use the `wgpu` crate to enumerate GPUs,
    // label them as CPU, Integrated GPU, and Discrete GPU
    // and return true only if one of them is a discrete gpu.
    // TODO: how does this act on macos?
    wgpu::Instance::default()
        .enumerate_adapters(wgpu::Backends::all())
        .into_iter()
        .any(|a| a.get_info().device_type == wgpu::DeviceType::DiscreteGpu)
}

pub fn get_model(model_path: &str) -> Result<Arc<LlamaModel>, String> {
    // HACK: only offload anything to the gpu if we can find a dedicated GPU
    //       there seems to be a bug which results in garbage tokens if we over-allocate an integrated GPU
    //       while using the vulkan backend. See: https://github.com/nobodywho-ooo/nobodywho-rs/pull/14
    println!("loading model from disk");
    if !std::path::Path::new(model_path).exists() {
        return Err(format!("{model_path} does not exist."));
    }

    let model_params = LlamaModelParams::default().with_n_gpu_layers(
        if has_discrete_gpu() || cfg!(target_os = "macos") {
            1000000
        } else {
            0
        },
    );
    let model_params = pin!(model_params);
    let model = LlamaModel::load_from_file(&LLAMA_BACKEND, model_path, &model_params)
        .map_err(|_| format!("Looks like {model_path} is not a valid or supported GGUF model."))?;
    println!("loaded model from disk");
    Ok(Arc::new(model))
}

// random candidates

pub fn apply_chat_template(model: Model, chat: Vec<(String, String)>) -> Result<String, String> {
    let chat_result: Result<Vec<LlamaChatMessage>, String> = chat
        .into_iter()
        .map(|t| LlamaChatMessage::new(t.0, t.1).map_err(|e| e.to_string()))
        .collect();
    let chat_string = model
        .apply_chat_template(None, chat_result?, true)
        .map_err(|e| e.to_string())?;
    Ok(chat_string)
}

pub struct SamplerConfig {
    pub seed: u32,
    pub temperature: f32,
    pub penalty_last_n: i32,
    pub penalty_repeat: f32,
    pub penalty_freq: f32,
    pub penalty_present: f32,
    pub penalize_nl: bool,
    pub ignore_eos: bool,
    pub mirostat_tau: f32,
    pub mirostat_eta: f32,
}

// defaults here match the defaults read from `llama-cli --help`
pub const DEFAULT_SAMPLER_CONFIG: SamplerConfig = SamplerConfig {
    seed: 1234,
    temperature: 0.8,
    penalty_last_n: -1,
    penalty_repeat: 0.0,
    penalty_freq: 0.0,
    penalty_present: 0.0,
    penalize_nl: false,
    ignore_eos: false,
    mirostat_tau: 5.0,
    mirostat_eta: 0.1,
};

fn make_sampler(
    model: &LlamaModel,
    config: SamplerConfig,
) -> Result<LlamaSampler, LlamaSamplerError> {
    // init mirostat sampler
    let sampler_params = LlamaSamplerChainParams::default();
    let sampler = LlamaSampler::new(sampler_params)?
        .add_penalties(
            model.n_vocab(),
            model.token_eos().0,
            model.token_nl().0,
            config.penalty_last_n,
            config.penalty_repeat,
            config.penalty_freq,
            config.penalty_present,
            config.penalize_nl,
            config.ignore_eos,
        )
        .add_temp(config.temperature)
        .add_mirostat_v2(config.seed, config.mirostat_tau, config.mirostat_eta);
    Ok(sampler)
}

pub fn run_worker<'a>(
    model: Arc<LlamaModel>,
    prompt_rx: Receiver<String>,
    completion_tx: Sender<LLMOutput>,
    sampler_config: SamplerConfig,
) {
    println!("in worker thread");
    let n_threads = std::thread::available_parallelism().unwrap().get() as i32;
    let ctx_params = LlamaContextParams::default()
        .with_n_ctx(std::num::NonZero::new(model.n_ctx_train()))
        .with_n_threads(n_threads)
        .with_n_threads_batch(n_threads);

    println!("making context");
    let mut ctx = model.new_context(&LLAMA_BACKEND, ctx_params).unwrap();
    println!("made context");

    let mut n_cur = 0;

    println!("making sampler");
    let mut sampler = make_sampler(&model, sampler_config)
        .expect("Llama.cpp returned a null pointer when initializing sampler.");
    println!("made sampler");

    println!("worker getting prompt");
    while let Ok(prompt) = prompt_rx.recv() {
        println!("worker got prompt");

        println!("worker making tokens list");
        let tokens_list = ctx.model.str_to_token(&prompt, AddBos::Always).unwrap();
        println!("worker made tokens list");
        println!("got {:?} tokens", tokens_list.len());

        println!("making batch");
        let mut batch = LlamaBatch::new(ctx.n_ctx() as usize, 1);
        println!("made batch");

        // put tokens in the batch
        let last_index = (tokens_list.len() - 1) as i32;

        println!("adding to batch");
        for (i, token) in (0..).zip(tokens_list.into_iter()) {
            // llama_decode will output logits only for the last token of the prompt
            let is_last = i == last_index;
            batch.add(token, n_cur + i, &[0], is_last).unwrap();
        }
        println!("added to batch");

        println!("decoding");
        ctx.decode(&mut batch).unwrap();
        println!("decoded");

        // main loop
        n_cur += batch.n_tokens();

        // The `Decoder`
        let mut utf8decoder = encoding_rs::UTF_8.new_decoder();

        loop {
            // sample the next token
            {
                // sample the next token
                println!("sampling token");
                let new_token_id: LlamaToken = sampler.sample(&ctx, batch.n_tokens() - 1);
                println!("sampled token");
                sampler.accept(new_token_id);

                // is it an end of stream?
                println!("checking end of stream");
                if new_token_id == ctx.model.token_eos() {
                    batch.clear();
                    batch.add(new_token_id, n_cur, &[0], true).unwrap();
                    completion_tx.send(LLMOutput::Done).unwrap();
                    println!("end of stream reached");
                    break;
                }
                println!("checked end of stream");

                println!("making new tokens bytes");
                let output_bytes = ctx
                    .model
                    .token_to_bytes(new_token_id, Special::Tokenize)
                    .unwrap();
                println!("got new token bytes");

                // use `Decoder.decode_to_string()` to avoid the intermediate buffer
                let mut output_string = String::with_capacity(32);
                let _decode_result =
                    utf8decoder.decode_to_string(&output_bytes, &mut output_string, false);
                println!("got new token string: {:?}", output_string);

                // send new token string back to user
                println!("sending string back");
                completion_tx.send(LLMOutput::Token(output_string)).unwrap();
                println!("sent string back");

                // prepare batch or the next decode
                println!("clearing batch");
                batch.clear();
                println!("clared batch");

                println!("adding new token to batch");
                batch.add(new_token_id, n_cur, &[0], true).unwrap();
                println!("added new token to batch");
            }

            n_cur += 1;

            println!("decoding");
            ctx.decode(&mut batch).unwrap();
            println!("decoded");
        }
    }
}

macro_rules! test_model_path {
    () => {
        std::env::var("TEST_MODEL")
            .unwrap_or("model.gguf".to_string())
            .as_str()
    };
}

#[cfg(test)]
mod tests {
    use llama_cpp_2::model::LlamaChatMessage;

    use super::*;

    #[test]
    fn test_completion() {
        let model = get_model(test_model_path!()).unwrap();

        let (prompt_tx, prompt_rx) = std::sync::mpsc::channel();
        let (completion_tx, completion_rx) = std::sync::mpsc::channel();

        std::thread::spawn(move || {
            run_worker(model, prompt_rx, completion_tx, DEFAULT_SAMPLER_CONFIG)
        });

        prompt_tx.send("Count to five: 1, 2, ".to_string()).unwrap();

        let mut result: String = "".to_string();

        for _ in 0..10 {
            match completion_rx.recv() {
                Ok(LLMOutput::Token(token)) => {
                    result += token.as_str();
                }
                Ok(LLMOutput::Done) => {
                    break;
                }
                _ => unreachable!(),
            }
        }
        let expected = "3, 4, 5";
        assert_eq!(&result[..expected.len()], expected);

        // Kill worker
    }

    #[test]
    fn test_chat_completion() {
        let model = get_model(test_model_path!()).unwrap();
        let model_copy = model.clone();

        let (prompt_tx, prompt_rx) = std::sync::mpsc::channel();
        let (completion_tx, completion_rx) = std::sync::mpsc::channel();

        std::thread::spawn(|| run_worker(model, prompt_rx, completion_tx, DEFAULT_SAMPLER_CONFIG));

        let chat: Vec<LlamaChatMessage> = vec![
            LlamaChatMessage::new(
                "system".to_string(),
                "You are a helpful assistant. The user asks you a question, and you provide an answer. You take multiple turns to provide the answer. Be consice and only provide the answer".to_string(),
            )
            .unwrap(),
            LlamaChatMessage::new(
                "user".to_string(),
                "What is the capital of Denmark?".to_string(),
            )
            .unwrap(),
        ];

        let prompt = model_copy.apply_chat_template(None, chat, true).unwrap();

        prompt_tx.send(prompt).unwrap();

        let mut result: String = "".to_string();

        loop {
            match completion_rx.recv() {
                Ok(LLMOutput::Token(token)) => {
                    result += token.as_str();
                }
                Ok(LLMOutput::Done) => {
                    break;
                }
                _ => unreachable!(),
            }
        }

        let chat: Vec<LlamaChatMessage> = vec![LlamaChatMessage::new(
            "user".to_string(),
            "What language to they speak there?".to_string(),
        )
        .unwrap()];

        let prompt = model_copy.apply_chat_template(None, chat, true).unwrap();

        prompt_tx.send(prompt).unwrap();

        let mut result: String = "".to_string();

        loop {
            match completion_rx.recv() {
                Ok(LLMOutput::Token(token)) => {
                    result += token.as_str();
                }
                Ok(LLMOutput::Done) => {
                    break;
                }
                _ => unreachable!(),
            }
        }

        assert!(
            result.contains("Danish"),
            "Expected completion to contain 'Danish', got: {result}"
        );
    }

    #[test]
    fn test_initialize_default_sampler() {
        let model = get_model(test_model_path!()).expect("Failed loading model");
        let sampler = make_sampler(&model, DEFAULT_SAMPLER_CONFIG);
        assert!(sampler.is_ok(), "make_sampler returned an Err");
    }
}
