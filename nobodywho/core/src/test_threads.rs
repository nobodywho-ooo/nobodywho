use llama_cpp_2::{
    context::params::LlamaContextParams,
    model::params::LlamaModelParams,
    model::{AddBos, LlamaModel},
    llama_backend::LlamaBackend,
    llama_batch::LlamaBatch,
};

pub static LLAMA_BACKEND: LazyLock<LlamaBackend> = LazyLock::new(|| LlamaBackend::init().expect("Failed to initialize llama backend"));



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_threading() {
        let backend = LlamaBackend::init().expect("Failed to init backend");
        let model_path = "model.gguf"; // Adjust path as needed
    
        // Thread 1 - just loads model
        let backend_clone = backend.clone();
        let thread1 = std::thread::spawn(move || {
            eprintln!("Thread 1: Loading model");
            let model_params = LlamaModelParams::default();
            let _model = LlamaModel::load_from_file(&backend_clone, model_path, &model_params)
                .expect("Failed to load model in thread 1");
            std::thread::sleep(std::time::Duration::from_secs(2));
            println!("Thread 1: Done");
        });

        // Thread 2 - loads and uses model
        let thread2 = std::thread::spawn(move || {
            println!("Thread 2: Loading model");
            let model_params = LlamaModelParams::default();
            let model = LlamaModel::load_from_file(&backend, model_path, &model_params)
                .expect("Failed to load model in thread 2");

            println!("Thread 2: Creating context");
            let mut ctx = model.new_context(&backend, LlamaContextParams::default())
                .expect("Failed to create context");

            // Simple inference
            println!("Thread 2: Running inference");
            let text = "Say hello in English: ";
            let tokens = model.str_to_token(text, AddBos::Always)
                .expect("Failed to tokenize");

            let mut batch = LlamaBatch::new(ctx.n_ctx() as usize, 1);
            for (i, &token) in tokens.iter().enumerate() {
                batch.add(token, i as i32, &[0], true)
                    .expect("Failed to add token to batch");
            }

            ctx.decode(&mut batch).expect("Failed to decode");
            println!("Thread 2: Done");
        });
        thread1.join().unwrap();
        thread2.join().unwrap();
    }
}