const CHANNEL_SIZE: usize = 4096; // this number is very arbitrary

/// Parameters for configuring an LLM actor instance.
///
/// This struct contains the configuration needed to create a new LLM actor,
/// including the model, sampling parameters, context size, and stop tokens.
///
/// # Fields
/// * `model` - The LLaMA model to use for inference, wrapped in an Arc for thread-safe sharing
/// * `sampler_config` - Configuration for the token sampling strategy
/// * `n_ctx` - Maximum context length in tokens
/// * `stop_tokens` - List of strings that will cause token generation to stop when encountered
#[derive(Clone)]
pub struct LLMActorParams {
    pub model: Arc<LlamaModel>,
    pub sampler_config: SamplerConfig,
    pub n_ctx: u32,
    pub stop_tokens: Vec<String>,
}

#[derive(Debug)]
pub struct LLMActorHandle {
    message_tx: std::sync::mpsc::Sender<WorkerMsg>,
}

impl LLMActorHandle {
    #[tracing::instrument(level = "debug", skip(params))]
    pub async fn new(params: LLMActorParams) -> Result<Self, InitWorkerError> {
        debug!("Creating LLM actor");

        let (message_tx, message_rx) = std::sync::mpsc::channel();
        let (init_tx, init_rx) = oneshot::channel();

        std::thread::spawn(move || completion_worker_actor(message_rx, init_tx, params));

        debug!("Waiting for worker initialization");
        let result = match init_rx.await {
            Ok(Ok(())) => {
                info!("LLM actor initialized successfully");
                Ok(Self { message_tx })
            }
            Ok(Err(e)) => {
                error!(error = ?e, "LLM actor initialization failed");
                Err(e)
            }
            Err(_) => {
                error!("No response from worker thread during initialization");
                Err(InitWorkerError::NoResponse)
            }
        };

        result
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn reset_context(&self) -> Result<(), oneshot::error::RecvError> {
        debug!("Resetting context");
        let (respond_to, response) = oneshot::channel();
        let _ = self.message_tx.send(WorkerMsg::ResetContext(respond_to));
        let result = response.await;
        if result.is_ok() {
            debug!("Context reset successful");
        } else {
            error!("Context reset failed");
        }
        result
    }

    #[tracing::instrument(level = "debug", skip(self), fields(text_length = text.len()))]
    pub async fn read(
        &self,
        text: String,
    ) -> Result<Result<(), ReadError>, oneshot::error::RecvError> {
        debug!("Reading text into context");
        let (respond_to, response_channel) = oneshot::channel();
        let _ = self
            .message_tx
            .send(WorkerMsg::ReadString(text, respond_to));

        let result = response_channel.await;
        match &result {
            Ok(Ok(_)) => debug!("Successfully read text into context"),
            Ok(Err(e)) => error!(error = ?e, "Failed to read text into context"),
            Err(_) => error!("Worker died while reading text"),
        }
        result
    }

    pub async fn write_until_done(
        &self,
    ) -> tokio_stream::wrappers::ReceiverStream<Result<WriteOutput, WriteError>> {
        let (respond_to, response_channel) = mpsc::channel(CHANNEL_SIZE);
        let _ = self.message_tx.send(WorkerMsg::WriteUntilDone(respond_to));
        response_channel.into()
    }

    pub async fn get_embedding(
        &self,
    ) -> Result<Result<Vec<f32>, llama_cpp_2::EmbeddingsError>, oneshot::error::RecvError> {
        let (respond_to, response_channel) = oneshot::channel();
        let _ = self.message_tx.send(WorkerMsg::GetEmbedding(respond_to));
        response_channel.await
    }

    pub async fn generate_response(
        &self,
        text: String,
    ) -> tokio_stream::wrappers::ReceiverStream<Result<WriteOutput, GenerateResponseError>> {
        let (respond_to, response_channel) = mpsc::channel(CHANNEL_SIZE);
        let _ = self
            .message_tx
            .send(WorkerMsg::GenerateResponse(text, respond_to));
        response_channel.into()
    }

    pub async fn generate_embedding(
        &self,
        text: String,
    ) -> Result<Vec<f32>, GenerateEmbeddingError> {
        let (respond_to, response_channel) = oneshot::channel();
        let _ = self
            .message_tx
            .send(WorkerMsg::GenerateEmbedding(text, respond_to));
        response_channel.await?
    }
}

fn completion_worker_actor(
    message_rx: std::sync::mpsc::Receiver<WorkerMsg>,
    init_tx: oneshot::Sender<Result<(), InitWorkerError>>,
    params: LLMActorParams,
) {
    match WorkerState::new(&params) {
        Ok(mut state) => {
            let _ = init_tx.send(Ok(())); // no way to recover from this send error

            // listen for messages forever
            while let Ok(msg) = message_rx.recv() {
                match handle_msg(state, msg) {
                    Ok(newstate) => {
                        state = newstate;
                    }
                    Err(()) => {
                        error!("Failed handling message");
                        return; // we died.
                    }
                }
            } // message queue dropped. we died.
        }
        Err(initerr) => {
            error!("Init WorkerState failure.");
            let _ = init_tx.send(Err(initerr));
            // we died. not much to do.
        }
    }
}

#[derive(Debug)]
pub enum WorkerMsg {
    ReadString(String, oneshot::Sender<Result<(), ReadError>>),
    WriteUntilDone(mpsc::Sender<Result<WriteOutput, WriteError>>),
    GetEmbedding(oneshot::Sender<Result<Vec<f32>, llama_cpp_2::EmbeddingsError>>),
    ResetContext(oneshot::Sender<()>),
    GenerateResponse(
        String,
        mpsc::Sender<Result<WriteOutput, GenerateResponseError>>,
    ),
    GenerateEmbedding(
        String,
        oneshot::Sender<Result<Vec<f32>, GenerateEmbeddingError>>,
    ),
}

fn handle_msg(state: WorkerState, msg: WorkerMsg) -> Result<WorkerState, ()> {
    // HACK
    // this is needed because contexts referencing the same model are not thread safe
    // if two contexts referencing the same model try to decode at the same time,
    // then llama.cpp segfaults and everybody dies and i become sad
    debug!("Worker handling message: {msg:?}");
    let _inference_lock = GLOBAL_INFERENCE_LOCK.lock().expect("GIL mutex poisoned.");

    match msg {
        WorkerMsg::ReadString(text, respond_to) => match state.read_string(text) {
            Ok(newstate) => {
                let _ = respond_to.send(Ok(()));
                Ok(newstate)
            }
            Err(e) => {
                let _ = respond_to.send(Err(e));
                Err(())
            }
        },
        WorkerMsg::WriteUntilDone(respond_to) => state
            .write_until_done(|out| {
                let _ = respond_to.blocking_send(Ok(out));
            })
            .map_err(|e| {
                let _ = respond_to.blocking_send(Err(e.into()));
                ()
            }),
        WorkerMsg::GetEmbedding(respond_to) => match state.ctx.embeddings_seq_ith(0) {
            Ok(embd) => {
                let _ = respond_to.send(Ok(embd.to_vec()));
                Ok(state)
            }
            Err(e) => {
                let _ = respond_to.send(Err(e.into()));
                Err(())
            }
        },
        WorkerMsg::ResetContext(respond_to) => {
            let new_state = state.reset_context();
            let _ = respond_to.send(());
            Ok(new_state)
        }
        // read then write text until done
        WorkerMsg::GenerateResponse(text, respond_to) => state
            .read_string(text)
            .map_err(|e| {
                let _ = respond_to.blocking_send(Err(e.into()));
                ()
            })?
            .write_until_done(|out| {
                let _ = respond_to.blocking_send(Ok(out));
            })
            .map_err(|e| {
                let _ = respond_to.blocking_send(Err(e.into()));
                ()
            }),
        // read string then retrieve embedding
        WorkerMsg::GenerateEmbedding(text, respond_to) => {
            // try reading the string
            let state = match state.read_string(text) {
                Ok(new_state) => new_state,
                Err(e) => {
                    // error and return early, moving respond_to only once
                    let _ = respond_to.send(Err(e.into()));
                    return Err(());
                }
            };

            // try getting embeddings
            match state.ctx.embeddings_seq_ith(0) {
                Ok(embd) => {
                    // success!
                    let _ = respond_to.send(Ok(embd.to_vec()));
                    Ok(state.reset_context())
                }
                Err(e) => {
                    // :(
                    let _ = respond_to.send(Err(e.into()));
                    Err(())
                }
            }
        }
    }
}
