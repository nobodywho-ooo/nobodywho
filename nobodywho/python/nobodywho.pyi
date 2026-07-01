import os
import typing
from collections.abc import Sequence
from os import PathLike
from pathlib import Path
from typing import final

T = typing.TypeVar(
    "T", str, typing.Awaitable[str]
)  # Type variable for tool return types (sync str or async Awaitable[str])

@final
class Audio:
    """
    An `Audio` prompt part, used to build multimodal `Prompt`s.

    Example:
        prompt = Prompt([Text("Transcribe this:"), Audio("./clip.wav")])
    """
    def __new__(cls, /, path: "os.PathLike | str") -> "Audio": ...
    def __repr__(self, /) -> str: ...
    @property
    def path(self, /) -> str: ...

@final
class Chat:
    """
    `Chat` is a general-purpose class for interacting with instruction-tuned conversational LLMs.
    It should be initialized with a turn-taking LLM, which includes a chat template.
    On a `Chat` instance, you can call `.ask()` with the prompt you intend to pass to the model,
    which returns a `TokenStream`, representing the generated response.
    `Chat` also supports calling tools.
    When initializing a `Chat`, you can also specify additional generation configuration, like
    what tools to provide, what sampling strategy to use for choosing tokens, what system prompt
    to use, whether to allow extended thinking, etc.
    See `ChatAsync` for the async version of this class.
    """
    def __new__(
        cls,
        /,
        model: "Model | os.PathLike | str",
        n_ctx: int = 4096,
        system_prompt: str | None = None,
        template_variables: "dict[str, bool]" = ...,
        tools: "list[Tool]" = ...,
        sampler: "SamplerConfig | None" = None,
        allow_thinking: "bool | None" = None,
    ) -> "Chat":
        """
        Create a new Chat instance for conversational text generation.

        Args:
            model: A chat model (Model instance, local path, `huggingface:` path, or `https://` URL to a GGUF file)
            n_ctx: Context size (maximum conversation length in tokens). Defaults to 4096.
            system_prompt: System message to guide the model's behavior. Defaults to empty string.
            template_variables: Dict of template variables to pass to the chat template (e.g., {"enable_thinking": True}). Defaults to empty dict.
            tools: List of Tool instances the model can call. Defaults to empty list.
            sampler: SamplerConfig for token selection. If not given, sampling settings
                embedded in the model file (general.sampling.* metadata) are used when
                present, otherwise SamplerConfig.default().
            allow_thinking: DEPRECATED. Use template_variables={"enable_thinking": True} instead. If set, overrides enable_thinking in template_variables.

        Returns:
            A Chat instance

        Raises:
            RuntimeError: If the model cannot be loaded
        """
    def ask(self, /, prompt: "str | Prompt") -> "TokenStream":
        """
        Send a message to the model and get a streaming response.

        Args:
            prompt: The user prompt to send (plain text or a multimodal Prompt)

        Returns:
            A TokenStream that yields tokens as they are generated
        """
    def get_chat_history(self, /) -> "list[dict]":
        """
        Get the current chat history as a list of message dictionaries.

        Returns:
            List of message dicts, each with 'role' (str) and 'content' (str) keys.
            Example: [{"role": "user", "content": "Hello"}, {"role": "assistant", "content": "Hi!"}]

        Raises:
            RuntimeError: If retrieval fails
        """
    def get_sampler_config(self, /) -> SamplerConfig:
        """
        Get the current sampler configuration.

        Returns:
            The current SamplerConfig used for token selection

        Raises:
            RuntimeError: If the sampler config cannot be retrieved
        """
    def get_system_prompt(self, /) -> str | None:
        """
        Get the current system prompt.

        Returns:
            The current system prompt, or None if not set

        Raises:
            RuntimeError: If the system prompt cannot be retrieved
        """
    def get_template_variables(self, /) -> dict[str, bool]:
        """
        Get all template variables.

        Returns:
            Dict of template variable names to boolean values

        Raises:
            RuntimeError: If the variables cannot be retrieved
        """
    def reset(self, /, system_prompt: str | None, tools: Sequence[Tool]) -> None:
        """
        Reset the conversation with a new system prompt and tools. Clears all chat history.

        Args:
            system_prompt: New system message to guide the model's behavior
            tools: New list of Tool instances the model can call

        Raises:
            RuntimeError: If reset fails
        """
    def reset_history(self, /) -> None:
        """
        Clear the chat history while keeping the system prompt and tools unchanged.

        Raises:
            RuntimeError: If reset fails
        """
    def set_allow_thinking(self, /, allow_thinking: bool) -> None:
        """
        DEPRECATED: Use set_template_variable("enable_thinking", value) instead.

        Enable or disable extended reasoning tokens for supported models.

        Args:
            allow_thinking: If True, allows extended reasoning tokens

        Raises:
            ValueError: If the setting cannot be changed
        """
    def set_chat_history(self, /, msgs: "list[dict]") -> "None":
        """
        Replace the chat history with a new list of messages.

        Args:
            msgs: List of message dicts, each with 'role' (str) and 'content' (str) keys.
                  Example: [{"role": "user", "content": "Hello"}, {"role": "assistant", "content": "Hi!"}]

        Raises:
            ValueError: If message format is invalid
            RuntimeError: If setting history fails
        """
    def set_sampler_config(self, /, sampler: SamplerConfig) -> None:
        """
        Update the sampler configuration without resetting chat history.

        Args:
            sampler: New SamplerConfig for token selection

        Raises:
            RuntimeError: If the sampler config cannot be changed
        """
    def set_system_prompt(self, /, system_prompt: str | None) -> None:
        """
        Update the system prompt without resetting chat history.

        Args:
            system_prompt: New system message to guide the model's behavior

        Raises:
            RuntimeError: If the system prompt cannot be changed
        """
    def set_template_variable(self, /, name: str, value: bool) -> None:
        """
        Set a single template variable

        Args:
            name: The name of the template variable (e.g., "enable_thinking")
            value: The boolean value for the variable

        Raises:
            RuntimeError: If the variable cannot be set
        """
    def set_template_variables(self, /, variables: dict[str, bool]) -> None:
        """
        Set all template variables, replacing any existing ones.

        Args:
            variables: Dict of template variable names to boolean values

        Raises:
            RuntimeError: If the variables cannot be set
        """
    def set_tools(self, /, tools: Sequence[Tool]) -> None:
        """
        Update the list of tools available to the model without resetting chat history.

        Args:
            tools: New list of Tool instances the model can call

        Raises:
            RuntimeError: If updating tools fails
        """
    def stats(self, /) -> "ChatStats":
        """
        Get context usage statistics.

        Returns:
            ChatStats with context_size and context_used fields
        """
    def stop_generation(self, /) -> None:
        """
        Stop the current text generation immediately.

        This can be used to cancel an in-progress generation if the response is taking too long
        or is no longer needed.
        """
    def tokenize(self, /, prompt: "str | Prompt") -> "list[int | None]":
        """
        Tokenize a prompt and return token IDs.

        Text tokens are returned as integers. Media embedding slots (images, audio)
        are returned as None — one None per context slot consumed.

        Note: tokenizing a prompt with images requires loading and processing those
        images through the projection model, so it is not a free operation.

        Args:
            prompt: The text or multimodal Prompt to tokenize

        Returns:
            list[int | None] — token IDs for text, None for each media embedding slot

        Raises:
            RuntimeError: If tokenization fails
        """

@final
class ChatAsync:
    """
    This is the async version of the `Chat` class.
    See the docs for the `Chat` class for more information.
    """
    def __new__(
        cls,
        /,
        model: "Model | os.PathLike | str",
        n_ctx: int = 4096,
        system_prompt: str | None = None,
        template_variables: "dict[str, bool]" = ...,
        tools: "list[Tool]" = ...,
        sampler: "SamplerConfig | None" = None,
        allow_thinking: "bool | None" = None,
    ) -> "ChatAsync":
        """
        Create a new async Chat instance for conversational text generation.

        Args:
            model: A chat model (Model instance, local path, `huggingface:` path, or `https://` URL to a GGUF file)
            n_ctx: Context size (maximum conversation length in tokens). Defaults to 4096.
            system_prompt: System message to guide the model's behavior. Defaults to empty string.
            template_variables: Dict of template variables to pass to the chat template (e.g., {"enable_thinking": True}). Defaults to empty dict.
            tools: List of Tool instances the model can call. Defaults to empty list.
            sampler: SamplerConfig for token selection. If not given, sampling settings
                embedded in the model file (general.sampling.* metadata) are used when
                present, otherwise SamplerConfig.default().
            allow_thinking: DEPRECATED. Use template_variables={"enable_thinking": True} instead. If set, overrides enable_thinking in template_variables.

        Returns:
            A ChatAsync instance

        Raises:
            RuntimeError: If the model cannot be loaded
        """
    def ask(self, /, prompt: "str | Prompt") -> "TokenStreamAsync":
        """
        Send a message to the model and get a streaming response asynchronously.

        Args:
            prompt: The user prompt to send (plain text or a multimodal Prompt)

        Returns:
            A TokenStreamAsync that yields tokens as they are generated
        """
    async def get_chat_history(self, /) -> "list[dict]":
        """
        Get the current chat history as a list of message dictionaries.

        Returns:
            List of message dicts, each with 'role' (str) and 'content' (str) keys.
            Example: [{"role": "user", "content": "Hello"}, {"role": "assistant", "content": "Hi!"}]

        Raises:
            RuntimeError: If retrieval fails
        """
    async def get_sampler_config(self, /) -> SamplerConfig:
        """
        Get the current sampler configuration.

        Returns:
            The current SamplerConfig used for token selection

        Raises:
            RuntimeError: If the sampler config cannot be retrieved
        """
    async def get_system_prompt(self, /) -> str | None:
        """
        Get the current system prompt.

        Returns:
            The current system prompt, or None if not set

        Raises:
            RuntimeError: If the system prompt cannot be retrieved
        """
    async def get_template_variables(self, /) -> dict[str, bool]:
        """
        Get all template variables.

        Returns:
            Dict of template variable names to boolean values

        Raises:
            RuntimeError: If the variables cannot be retrieved
        """
    async def reset(self, /, system_prompt: str | None, tools: Sequence[Tool]) -> None:
        """
        Reset the conversation with a new system prompt and tools. Clears all chat history.

        Args:
            system_prompt: New system message to guide the model's behavior
            tools: New list of Tool instances the model can call

        Raises:
            RuntimeError: If reset fails
        """
    async def reset_history(self, /) -> None:
        """
        Clear the chat history while keeping the system prompt and tools unchanged.

        Raises:
            RuntimeError: If reset fails
        """
    async def set_allow_thinking(self, /, allow_thinking: bool) -> None:
        """
        DEPRECATED: Use set_template_variable("enable_thinking", value) instead.

        Enable or disable extended reasoning tokens for supported models.

        Args:
            allow_thinking: If True, allows extended reasoning tokens

        Raises:
            ValueError: If the setting cannot be changed
        """
    async def set_chat_history(self, /, msgs: "list[dict]") -> "None":
        """
        Replace the chat history with a new list of messages.

        Args:
            msgs: List of message dicts, each with 'role' (str) and 'content' (str) keys.
                  Example: [{"role": "user", "content": "Hello"}, {"role": "assistant", "content": "Hi!"}]

        Raises:
            ValueError: If message format is invalid
            RuntimeError: If setting history fails
        """
    async def set_sampler_config(self, /, sampler: SamplerConfig) -> None:
        """
        Update the sampler configuration without resetting chat history.

        Args:
            sampler: New SamplerConfig for token selection

        Raises:
            RuntimeError: If the sampler config cannot be changed
        """
    async def set_system_prompt(self, /, system_prompt: str | None) -> None:
        """
        Update the system prompt without resetting chat history.

        Args:
            system_prompt: New system message to guide the model's behavior

        Raises:
            RuntimeError: If the system prompt cannot be changed
        """
    async def set_template_variable(self, /, name: str, value: bool) -> None:
        """
        Set a single template variable.

        Args:
            name: The name of the template variable (e.g., "enable_thinking")
            value: The boolean value for the variable

        Raises:
            RuntimeError: If the variable cannot be set
        """
    async def set_template_variables(self, /, variables: dict[str, bool]) -> None:
        """
        Set all template variables, replacing any existing ones.

        Args:
            variables: Dict of template variable names to boolean values

        Raises:
            RuntimeError: If the variables cannot be set
        """
    async def set_tools(self, /, tools: Sequence[Tool]) -> None:
        """
        Update the list of tools available to the model without resetting chat history.

        Args:
            tools: New list of Tool instances the model can call

        Raises:
            RuntimeError: If updating tools fails
        """
    async def stats(self, /) -> "ChatStats":
        """
        Get context usage statistics.

        Returns:
            ChatStats with context_size and context_used fields
        """
    async def stop_generation(self, /) -> None:
        """
        Stop the current text generation immediately.

        This can be used to cancel an in-progress generation if the response is taking too long
        or is no longer needed.
        """
    async def tokenize(self, /, prompt: "str | Prompt") -> "list[int | None]":
        """
        Tokenize a prompt and return token IDs.

        Text tokens are returned as integers. Media embedding slots (images, audio)
        are returned as None — one None per context slot consumed.

        Note: tokenizing a prompt with images requires loading and processing those
        images through the projection model, so it is not a free operation.

        Args:
            prompt: The text or multimodal Prompt to tokenize

        Returns:
            list[int | None] — token IDs for text, None for each media embedding slot

        Raises:
            RuntimeError: If tokenization fails
        """

@final
class ChatStats:
    """
    Context usage statistics returned by `Chat.stats()` and `ChatAsync.stats()`.
    """
    def __repr__(self, /) -> str: ...
    @property
    def context_size(self, /) -> int:
        """
        The maximum number of tokens the context window can hold.
        """
    @property
    def context_used(self, /) -> int:
        """
        The number of tokens currently used in the context (KV cache position).
        """

@final
class CrossEncoder:
    """
    A `CrossEncoder` is a kind of encoder that is trained to compare similarity between two texts.
    It is particularly useful for searching a list of texts with a query, to find the closest one.
    `CrossEncoder` requires a model made specifically for cross-encoding.
    See `CrossEncoderAsync` for the async version of this class.
    """
    def __new__(
        cls, /, model: "Model | os.PathLike | str", n_ctx: int = 4096
    ) -> "CrossEncoder":
        """
        Create a new CrossEncoder for comparing text similarity.

        Args:
            model: A cross-encoder model (Model instance, local path, `huggingface:` path, or `https://` URL to a GGUF file)
            n_ctx: Context size (maximum sequence length). Defaults to 4096.

        Returns:
            A CrossEncoder instance

        Raises:
            RuntimeError: If the model cannot be loaded
        """
    def rank(self, /, query: str, documents: Sequence[str]) -> list[float]:
        """
        Compute similarity scores between a query and multiple documents. This method blocks.

        Args:
            query: The query text
            documents: List of documents to compare against the query

        Returns:
            List of similarity scores (higher = more similar). Scores are in the same order as documents.

        Raises:
            RuntimeError: If ranking fails
        """
    def rank_and_sort(
        self, /, query: str, documents: Sequence[str]
    ) -> list[tuple[str, float]]:
        """
        Rank documents by similarity to query and return them sorted. This method blocks.

        Args:
            query: The query text
            documents: List of documents to compare against the query

        Returns:
            List of (document, score) tuples sorted by descending similarity (most similar first).

        Raises:
            RuntimeError: If ranking fails
        """

@final
class CrossEncoderAsync:
    """
    This is the async version of `CrossEncoder`.
    See the docs for `CrossEncoder` for more details.
    """
    def __new__(
        cls, /, model: "Model | os.PathLike | str", n_ctx: int = 4096
    ) -> "CrossEncoderAsync":
        """
        Create a new async CrossEncoder for comparing text similarity.

        Args:
            model: A cross-encoder model (Model instance, local path, `huggingface:` path, or `https://` URL to a GGUF file)
            n_ctx: Context size (maximum sequence length). Defaults to 4096.

        Returns:
            A CrossEncoderAsync instance

        Raises:
            RuntimeError: If the model cannot be loaded
        """
    async def rank(self, /, query: str, documents: Sequence[str]) -> list[float]:
        """
        Compute similarity scores between a query and multiple documents asynchronously.

        Args:
            query: The query text
            documents: List of documents to compare against the query

        Returns:
            List of similarity scores (higher = more similar). Scores are in the same order as documents.

        Raises:
            RuntimeError: If ranking fails
        """
    async def rank_and_sort(
        self, /, query: str, documents: Sequence[str]
    ) -> list[tuple[str, float]]:
        """
        Rank documents by similarity to query and return them sorted asynchronously.

        Args:
            query: The query text
            documents: List of documents to compare against the query

        Returns:
            List of (document, score) tuples sorted by descending similarity (most similar first).

        Raises:
            RuntimeError: If ranking fails
        """

@final
class Encoder:
    """
    `Encoder` will let you generate vector representations of text.
    It must be initialized with a model that specifically supports generating embeddings.
    A regular chat/text-generation model will not just work.
    Once initialized, you can call `.encode()` on a string, which returns a list of 32-bit floats.
    See `EncoderAsync` for the async version of this class.
    """
    def __new__(
        cls, /, model: "Model | os.PathLike | str", n_ctx: int = 4096
    ) -> "Encoder":
        """
        Create a new Encoder for generating text embeddings.

        Args:
            model: An embedding model (Model instance, local path, `huggingface:` path, or `https://` URL to a GGUF file)
            n_ctx: Context size (maximum sequence length). Defaults to 4096.

        Returns:
            An Encoder instance

        Raises:
            RuntimeError: If the model cannot be loaded
        """
    def encode(self, /, text: str) -> list[float]:
        """
        Generate an embedding vector for the given text. This method blocks until complete.

        Args:
            text: The text to encode

        Returns:
            A list of floats representing the embedding vector

        Raises:
            RuntimeError: If encoding fails
        """

@final
class EncoderAsync:
    """
    This is the async version of the `Encoder` class. See the docs on `Encoder` for more detail.
    """
    def __new__(
        cls, /, model: "Model | os.PathLike | str", n_ctx: int = 4096
    ) -> "EncoderAsync":
        """
        Create a new async Encoder for generating text embeddings.

        Args:
            model: An embedding model (Model instance, local path, `huggingface:` path, or `https://` URL to a GGUF file)
            n_ctx: Context size (maximum sequence length). Defaults to 4096.

        Returns:
            An EncoderAsync instance

        Raises:
            RuntimeError: If the model cannot be loaded
        """
    async def encode(self, /, text: str) -> list[float]:
        """
        Generate an embedding vector for the given text asynchronously.

        Args:
            text: The text to encode

        Returns:
            A list of floats representing the embedding vector

        Raises:
            RuntimeError: If encoding fails
        """

@final
class Image:
    """
    An `Image` prompt part, used to build multimodal `Prompt`s.

    Example:
        prompt = Prompt([Text("Describe this"), Image("./img.jpg")])
    """
    def __new__(cls, /, path: "os.PathLike | str") -> "Image": ...
    def __repr__(self, /) -> str: ...
    @property
    def path(self, /) -> str: ...

@final
class Model:
    """
    `Model` objects contain a GGUF model. It is primarily useful for sharing a single model instance
    between multiple `Chat`, `Encoder`, or `CrossEncoder` instances.
    Sharing is efficient because the underlying model data is reference-counted.
    There is no `ModelAsync` variant. A regular `Model` can be used with both `Chat` and `ChatAsync`.
    """
    def __new__(
        cls,
        /,
        model_path: "os.PathLike | str",
        use_gpu_if_available: bool = True,
        projection_model_path: "os.PathLike | str | None" = None,
        on_download_progress: "typing.Callable[[int, int], None] | None" = None,
    ) -> "Model":
        """
        Create a new Model from a GGUF file.

        Args:
            model_path: Path or URL to a GGUF model file. Accepts a local file path (e.g. `./model.gguf`), a `huggingface:` path (e.g. `huggingface:owner/repo/file.gguf`), or an `https://` URL. Remote models are downloaded and cached automatically.
            use_gpu_if_available: If True, attempts to use GPU acceleration. Defaults to True.
            projection_model_path: Path or URL to a multimodal projector file for vision models. Accepts the same formats as model_path. Defaults to None.
            on_download_progress: Optional callable invoked during model downloads with `(downloaded_bytes, total_bytes)`. Not called for locally cached models. If a projection model is also downloaded, the callback fires for each download sequentially, so `total_bytes` resets between them. Defaults to None.

        Returns:
            A Model instance

        Raises:
            RuntimeError: If the model file cannot be loaded
        """
    @staticmethod
    async def load_model_async(
        model_path: "os.PathLike | str",
        use_gpu_if_available: bool = True,
        projection_model_path: "os.PathLike | str | None" = None,
        on_download_progress: "typing.Callable[[int, int], None] | None" = None,
    ) -> "Model":
        """
        Asynchronously load a model from a GGUF file.

        This static method loads a model asynchronously, which is useful for loading large models
        without blocking the async event loop. The blocking model load operation is offloaded to
        a background thread, allowing other async tasks to continue running.

        Args:
            model_path: Path or URL to a GGUF model file. Accepts a local file path (e.g. `./model.gguf`), a `huggingface:` path (e.g. `huggingface:owner/repo/file.gguf`), or an `https://` URL. Remote models are downloaded and cached automatically.
            use_gpu_if_available: If True, attempts to use GPU acceleration. Defaults to True.
            projection_model_path: Path or URL to a multimodal projector file for vision models. Accepts the same formats as model_path. Defaults to None.
            on_download_progress: Optional callable invoked during model downloads with `(downloaded_bytes, total_bytes)`. Not called for locally cached models. If a projection model is also downloaded, the callback fires for each download sequentially, so `total_bytes` resets between them. Defaults to None.

        Returns:
            A Model instance wrapped in an awaitable (async function returns a coroutine)

        Raises:
            RuntimeError: If the model file cannot be loaded
        """
    @property
    def max_ctx(self, /) -> int:
        """
        The maximum context size this model was trained with.
        """

@final
class Prompt:
    """
    A multimodal prompt consisting of interleaved `Text`, `Image`, and `Audio` parts.

    Example:
        prompt = Prompt([Text("Tell me what's in the image"), Image("./img.jpg")])
        prompt = Prompt.from_json({"role": "user", "content": "Hello"})
    """
    def __new__(cls, /, parts: "list[Text | Image | Audio]" = ...) -> "Prompt": ...
    @staticmethod
    def from_json(data: "object") -> "Prompt": ...

@final
class STT:
    """
    `STT` transcribes speech to text using a Whisper ONNX model.

    `source` is a HuggingFace repo ID (e.g. `"onnx-community/whisper-base"`) or
    a local directory path. `language` is an ISO 639-1 code (e.g. `"en"`);
    omit or pass `None` to auto-detect.

    Example::

        stt = nobodywho.STT("onnx-community/whisper-base")
        text = stt.transcribe_file("recording.mp3").completed()

        # Or stream tokens:
        for piece in stt.transcribe_file("recording.mp3"):
            print(piece, end="", flush=True)
    """
    def __new__(cls, /, source: str, language: str | None = None) -> STT: ...
    def transcribe_file(self, /, path: str) -> TokenStream:
        """
        Transcribe an audio file (WAV / MP3 / FLAC). Returns a `TokenStream`.
        """
    def transcribe_pcm(
        self, /, samples: Sequence[int], sample_rate: int
    ) -> TokenStream:
        """
        Transcribe raw i16 PCM samples from a microphone. Returns a `TokenStream`.
        """

@final
class STTAsync:
    """
    `STTAsync` is the async variant of `STT`.
    """
    def __new__(cls, /, source: str, language: str | None = None) -> STTAsync: ...
    def transcribe_file(self, /, path: str) -> TokenStreamAsync: ...
    def transcribe_pcm(
        self, /, samples: Sequence[int], sample_rate: int
    ) -> TokenStreamAsync: ...

@final
class SamplerBuilder:
    """
    `SamplerBuilder` is used to manually construct a sampler chain.
    A sampler chain consists of any number of probability-shifting steps, and a single sampling step.
    Probability-shifting steps are operations that transform the probability distribution of next
    tokens, as generated by the model. E.g. the top_k step will zero the probability of all tokens
    that aren't among the top K most probable (where K is some integer).
    A sampling step is a final step that selects a single token from the probability distribution
    that results from applying all of the probability-shifting steps in order.
    E.g. the `dist` sampling step selects a token with weighted randomness, and the
    `greedy` sampling step always selects the most probable.
    """
    def __new__(cls, /) -> SamplerBuilder:
        """
        Create a new SamplerBuilder to construct a custom sampler chain.
        """
    def dist(self, /) -> SamplerConfig:
        """
        Sample from the probability distribution (weighted random selection).

        Returns:
            A complete SamplerConfig ready to use
        """
    def dry(
        self,
        /,
        multiplier: float,
        base: float,
        allowed_length: int,
        penalty_last_n: int,
        seq_breakers: Sequence[str],
    ) -> SamplerBuilder:
        """
        DRY (Don't Repeat Yourself) sampler to reduce repetition.

        Args:
            multiplier: Penalty strength multiplier
            base: Base penalty value
            allowed_length: Maximum allowed repetition length
            penalty_last_n: Number of recent tokens to consider
            seq_breakers: List of strings that break repetition sequences
        """
    def grammar(
        self, /, grammar: str, trigger_on: str | None, root: str
    ) -> SamplerBuilder:
        """
        Apply a GBNF grammar constraint to enforce structured output.

        Deprecated: Use `SamplerPresets.constrain_with_grammar()` instead. It accepts both Lark and GBNF strings.

        Args:
            grammar: Grammar specification in GBNF format (GGML BNF, a variant of BNF used by llama.cpp)
            trigger_on: Optional string that, when generated, activates the grammar constraint.
                        Useful for letting the model generate free-form text until a specific marker.
            root: Name of the root grammar rule to start parsing from
        """
    def greedy(self, /) -> SamplerConfig:
        """
        Always select the most probable token (deterministic).

        Returns:
            A complete SamplerConfig ready to use
        """
    def min_p(self, /, min_p: float, min_keep: int) -> SamplerBuilder:
        """
        Keep tokens with probability above min_p * (probability of most likely token).

        Args:
            min_p: Minimum relative probability threshold (0.0 to 1.0). Typical: 0.05-0.1.
            min_keep: Minimum number of tokens to always keep
        """
    def mirostat_v1(self, /, tau: float, eta: float, m: int) -> SamplerConfig:
        """
        Use Mirostat v1 algorithm for perplexity-controlled sampling.
        Mirostat dynamically adjusts sampling to maintain a target "surprise" level,
        producing more coherent output than fixed temperature. Good for long-form generation.

        Args:
            tau: Target perplexity/surprise value (typically 3.0-5.0; lower = more focused)
            eta: Learning rate for perplexity adjustment (typically 0.1)
            m: Number of candidates to consider (typically 100)

        Returns:
            A complete SamplerConfig ready to use
        """
    def mirostat_v2(self, /, tau: float, eta: float) -> SamplerConfig:
        """
        Use Mirostat v2 algorithm for perplexity-controlled sampling.
        Mirostat v2 is a simplified version of Mirostat that's often preferred.
        It dynamically adjusts sampling to maintain a target "surprise" level.

        Args:
            tau: Target perplexity/surprise value (typically 3.0-5.0; lower = more focused)
            eta: Learning rate for perplexity adjustment (typically 0.1)

        Returns:
            A complete SamplerConfig ready to use
        """
    def penalties(
        self,
        /,
        penalty_last_n: int,
        penalty_repeat: float,
        penalty_freq: float,
        penalty_present: float,
    ) -> SamplerBuilder:
        """
        Apply repetition penalties to discourage repeated tokens.

        Args:
            penalty_last_n: Number of recent tokens to penalize (0 = disable)
            penalty_repeat: Base repetition penalty (1.0 = no penalty, >1.0 = penalize)
            penalty_freq: Frequency penalty based on token occurrence count
            penalty_present: Presence penalty for any token that appeared before
        """
    def seed(self, /, seed: int) -> SamplerBuilder:
        """
        Set the RNG seed used by random samplers (`dist`, `mirostat_v1`, `mirostat_v2`, `xtc`).
        `greedy` ignores it. If unset, a default seed is used.
        """
    def temperature(self, /, temperature: float) -> SamplerBuilder:
        """
        Apply temperature scaling to the probability distribution.

        Args:
            temperature: Temperature value (0.0 = deterministic, 1.0 = unchanged, >1.0 = more random)
        """
    def top_k(self, /, top_k: int) -> SamplerBuilder:
        """
        Keep only the top K most probable tokens. Typical values: 40-50.

        Args:
            top_k: Number of top tokens to keep
        """
    def top_p(self, /, top_p: float, min_keep: int) -> SamplerBuilder:
        """
        Keep tokens whose cumulative probability is below top_p. Typical values: 0.9-0.95.

        Args:
            top_p: Cumulative probability threshold (0.0 to 1.0)
            min_keep: Minimum number of tokens to always keep
        """
    def typical_p(self, /, typ_p: float, min_keep: int) -> SamplerBuilder:
        """
        Typical sampling: keeps tokens close to expected information content.

        Args:
            typ_p: Typical probability mass (0.0 to 1.0). Typical: 0.9.
            min_keep: Minimum number of tokens to always keep
        """
    def xtc(
        self, /, xtc_probability: float, xtc_threshold: float, min_keep: int
    ) -> SamplerBuilder:
        """
        XTC (eXclude Top Choices) sampler that probabilistically excludes high-probability tokens.
        This can increase output diversity by sometimes forcing the model to pick less obvious tokens.

        Args:
            xtc_probability: Probability of applying XTC on each token (0.0 to 1.0)
            xtc_threshold: Tokens with probability above this threshold may be excluded (0.0 to 1.0)
            min_keep: Minimum number of tokens to always keep (prevents excluding all tokens)
        """

@final
class SamplerConfig:
    """
    `SamplerConfig` contains the configuration for a token sampler. The mechanism by which
    NobodyWho will sample a token from the probability distribution, to include in the
    generation result.
    A `SamplerConfig` can be constructed either using a preset function from the `SamplerPresets`
    class, or by manually constructing a sampler chain using the `SamplerBuilder` class.
    `SamplerConfig` supports serialization to/from JSON via `to_json()` and `from_json()`.
    """
    def __repr__(self, /) -> str: ...
    @staticmethod
    def from_json(json_str: str) -> SamplerConfig:
        """
        Deserialize a sampler configuration from a JSON string.

        Args:
            json_str: A JSON string representing a sampler configuration

        Returns:
            A SamplerConfig instance

        Raises:
            ValueError: If the JSON is invalid or doesn't represent a valid sampler configuration
        """
    def to_json(self, /) -> str:
        """
        Serialize the sampler configuration to a JSON string.

        Returns:
            A JSON string representing this sampler configuration

        Raises:
            RuntimeError: If serialization fails
        """

@final
class SamplerPresets:
    """
    `SamplerPresets` is a static class which contains a bunch of functions to easily create a
    `SamplerConfig` from some pre-defined sampler chain.
    E.g. `SamplerPresets.temperature(0.8)` will return a `SamplerConfig` with temperature=0.8.
    """
    @staticmethod
    def constrain_with_grammar(grammar: str) -> SamplerConfig:
        """
        Create a sampler that constrains output using a Lark grammar via llguidance.

        Args:
            grammar: Lark grammar string
        """
    @staticmethod
    def constrain_with_json_schema(schema: str | dict) -> SamplerConfig:
        """
        Create a sampler that constrains output to a JSON schema via llguidance.

        Args:
            schema: JSON schema as a dict or a JSON string
        """
    @staticmethod
    def constrain_with_regex(pattern: str) -> SamplerConfig:
        """
        Create a sampler that constrains output to a regular expression via llguidance.

        Args:
            pattern: Regular expression pattern
        """
    @staticmethod
    def default() -> SamplerConfig:
        """
        Get the default sampler configuration.
        """
    @staticmethod
    def dry() -> SamplerConfig:
        """
        Create a DRY sampler preset to reduce repetition.
        """
    @staticmethod
    def grammar(grammar: str) -> SamplerConfig:
        """
        Deprecated: Use `SamplerPresets.constrain_with_grammar()` instead. It accepts both Lark and GBNF strings.
        """
    @staticmethod
    def greedy() -> SamplerConfig:
        """
        Create a greedy sampler (always picks most probable token).
        """
    @staticmethod
    def json() -> SamplerConfig:
        """
        Create a sampler that constrains output to valid JSON (any structure) using GBNF.

        For schema-validated JSON, use `constrain_with_json_schema()` instead.
        """
    @staticmethod
    def temperature(temperature: float) -> SamplerConfig:
        """
        Create a sampler with temperature scaling.

        Args:
            temperature: Temperature value (lower = more focused, higher = more random)
        """
    @staticmethod
    def top_k(top_k: int) -> SamplerConfig:
        """
        Create a sampler with top-k filtering only.

        Args:
            top_k: Number of top tokens to keep
        """
    @staticmethod
    def top_p(top_p: float) -> SamplerConfig:
        """
        Create a sampler with nucleus (top-p) sampling.

        Args:
            top_p: Cumulative probability threshold (0.0 to 1.0)
        """

@final
class Text:
    """
    A `Text` prompt part, used to build multimodal `Prompt`s.

    Example:
        prompt = Prompt([Text("Describe this"), Image("./img.jpg")])
    """
    def __new__(cls, /, text: str) -> Text: ...
    def __repr__(self, /) -> str: ...
    @property
    def text(self, /) -> str: ...

@final
class TokenStream:
    """
    `TokenStream` is returned by `Chat.ask`, `STT.transcribe_file`, and `STT.transcribe_pcm`.
    Iterate over it token-by-token or call `.completed()` for the full text at once.
    Also see `TokenStreamAsync` for the async variant.
    """
    def __iter__(self, /) -> TokenStream: ...
    def __next__(self, /) -> str: ...
    def completed(self, /) -> str: ...
    def next_token(self, /) -> str | None: ...

@final
class TokenStreamAsync:
    """
    `TokenStreamAsync` is the async variant of `TokenStream`.
    Supports `await stream.next_token()`, `await stream.completed()`, and `async for token in stream`.
    """
    def __aiter__(self, /) -> TokenStreamAsync: ...
    def __anext__(self, /) -> typing.Awaitable[str]: ...
    async def completed(self, /) -> str: ...
    async def next_token(self, /) -> str | None: ...

@final
class Tool(typing.Generic[T]):
    """
    A `Tool` is a wrapped python function, that can be passed as a tool for the model to call.
    `Tool`s are constructed using the `@tool` decorator.
    """
    def __call__(self, /, *args, **kwargs) -> "T": ...

@final
class Tts:
    """
    `Tts` synthesizes speech to WAV bytes.
    """
    def __new__(
        cls,
        /,
        source: "os.PathLike | str",
        backend: "typing.Literal['kokoro', 'supertonic'] | None" = None,
        voice: str | None = None,
        language: str | None = None,
        speed: float | None = None,
        steps: int | None = None,
        silence_duration: float | None = None,
        device: "typing.Literal['auto', 'cpu', 'cuda']" = "auto",
    ) -> "Tts":
        """
        Create a TTS synthesizer.

        Args:
            source: Local model directory or HuggingFace repo ID.
            backend: "kokoro" or "supertonic". Required for local or unknown sources.
                Known sources infer the backend when omitted.
            voice: Voice name. Backend default is used when omitted.
            language: Language code. Backend default is used when omitted.
            speed: Speaking speed. Backend default is used when omitted.
            steps: Supertonic denoising steps. Ignored by Kokoro.
            silence_duration: Supertonic silence between chunks in seconds.
            device: "auto", "cpu", or "cuda". Defaults to "auto".
        """
    def synthesize(self, /, text: str) -> bytes:
        """
        Synthesize text and return WAV bytes.
        """
    async def synthesize_async(self, /, text: str) -> bytes:
        """
        Synthesize text asynchronously and return WAV bytes.
        """

def bash_tool(max_commands: int | None = None) -> Tool:
    """
    Create a bash interpreter tool that the LLM can use to run bash snippets.

    Args:
        max_commands: Maximum number of commands the snippet may execute. Defaults to no limit.

    Returns:
        A Tool instance ready to pass to Chat or ChatAsync.
    """

def cleanup_logging() -> None: ...
def cosine_similarity(a: Sequence[float], b: Sequence[float]) -> float:
    """
    Compute the cosine similarity between two vectors.
    Particularly useful for comparing embedding vectors from an Encoder.

    Args:
        a: First vector
        b: Second vector (must have the same length as a)

    Returns:
        Similarity score between 0.0 and 1.0 (higher means more similar)

    Raises:
        ValueError: If vectors have different lengths
    """

def download_model(
    model_path: str | PathLike[str],
    headers: dict[str, str] | None = None,
    on_download_progress: "typing.Callable[[int, int], None] | None" = None,
) -> Path:
    """
    Download a model from a remote URL or HuggingFace path and return the local path.

    This is useful when you need to pass custom headers (e.g. for authentication).
    For unauthenticated downloads, you can pass the path directly to `Chat` or `Model`.

    Args:
        model_path: Path or URL to a GGUF model file. Accepts a local file path, a `huggingface:` path, or an `https://` URL.
        headers: Optional dict of HTTP headers to include in the download request (e.g. `{"Authorization": "Bearer hf_..."}`).
        on_download_progress: Optional callable invoked during downloads with `(downloaded_bytes, total_bytes)`.

    Returns:
        Local path to the downloaded model file, which can be passed to `Model` or `Chat`.

    Raises:
        RuntimeError: If the download fails
    """

def get_cached_models() -> list[tuple[str, int]]:
    """
    Returns every cached .gguf model paired with its byte size.

    Returns:
        list[tuple[str, int]]: each entry is (absolute path, size in bytes).

    Raises:
        RuntimeError: If the cache directory cannot be read
    """

def python_tool(
    max_duration: int | None = None,
    max_memory: int | None = None,
    max_recursion_depth: int | None = None,
) -> Tool:
    """
    Create a built-in tool that lets the LLM run sandboxed Python code.

    The model can call this tool to execute self-contained Python snippets via the Monty
    interpreter. No filesystem, network, or environment variable access is allowed unless
    explicitly passed as a hardcoded value.

    Args:
        max_duration: Maximum wall-clock seconds the snippet may run. Defaults to no limit.
        max_memory:   Maximum bytes of memory the snippet may allocate. Defaults to no limit.
        max_recursion_depth: Maximum call-stack depth. Defaults to no limit.

    Returns:
        A Tool instance ready to pass to Chat or ChatAsync.
    """

def tool(
    description: "str", params: "dict[str, str] | None" = None
) -> "typing.Callable[[typing.Callable[..., T]], Tool[T]]":
    """
    Decorator to convert a Python function into a Chat-compatible Tool instance.

    The decorated function will be callable by the model during chat. The model sees the
    function's name, description, and parameter types/descriptions to decide when to call it.

    Both synchronous and asynchronous functions are supported. Async functions are executed
    synchronously when called by the model.

    Args:
        description: A description of what the tool does (shown to the model)
        params: Optional dict mapping parameter names to their descriptions (shown to the model)

    Returns:
        A decorator that transforms a function into a Tool instance

    Examples:
        @tool("Get the current weather for a city", params={"city": "The city name"})
        def get_weather(city: str) -> str:
            return f"Weather in {city}: sunny"

        @tool("Fetch data from a URL", params={"url": "The URL to fetch"})
        async def fetch_url(url: str) -> str:
            import aiohttp
            async with aiohttp.ClientSession() as session:
                async with session.get(url) as response:
                    return await response.text()

    Note:
        All function parameters must have type hints. The function should return a string.
        Async functions (defined with 'async def') are automatically detected and handled.
    """
