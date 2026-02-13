use std::ffi::CString;
use std::sync::Arc;

use ahash::AHasher;
use llama_cpp_2::{
    model::{AddBos, LlamaModel},
    mtmd::{
        MtmdBitmap, MtmdContext, MtmdContextParams, MtmdInputChunkType, MtmdInputChunks,
        MtmdInputText,
    },
    token::LlamaToken,
};
use std::hash::{Hash, Hasher};
use tracing::{info, warn};

use crate::{errors::MultimodalError, errors::TokenizationError, llm::has_discrete_gpu};

#[derive(Clone, Debug)]
pub struct Prompt {
    parts: Vec<PromptPart>,
}

impl Prompt {
    pub fn new() -> Self {
        Self { parts: vec![] }
    }

    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        if let Some(PromptPart::Text(last_text)) = self.parts.last_mut() {
            last_text.push_str(&text.into());
        } else {
            self.parts.push(PromptPart::Text(text.into()));
        }

        self
    }

    pub fn with_image(mut self, image_path: impl Into<String>) -> Self {
        self.parts.push(PromptPart::Image(image_path.into()));
        self
    }

    pub fn to_string(&self) -> String {
        let marker = llama_cpp_2::mtmd::mtmd_default_marker();
        self.parts
            .iter()
            .map(|part| match part {
                PromptPart::Text(text) => text.clone(),
                PromptPart::Image(_) => marker.to_string(),
            })
            .collect::<Vec<String>>()
            .join("")
    }

    pub fn extract_paths(&self) -> Vec<String> {
        self.parts
            .iter()
            .filter_map(|part| match part {
                PromptPart::Image(path) => Some(path.clone()),
                PromptPart::Text(_) => None,
            })
            .collect()
    }
}

#[derive(Clone, Debug)]
enum PromptPart {
    Text(String),
    Image(String),
}

pub trait Promptable {
    fn to_prompt(&self) -> Prompt;
}

impl Promptable for String {
    fn to_prompt(&self) -> Prompt {
        Prompt {
            parts: vec![PromptPart::Text(self.clone())],
        }
    }
}

impl Promptable for Prompt {
    fn to_prompt(&self) -> Prompt {
        self.clone()
    }
}

impl Promptable for &str {
    fn to_prompt(&self) -> Prompt {
        Prompt {
            parts: vec![PromptPart::Text(self.to_string())],
        }
    }
}

impl From<String> for Prompt {
    fn from(s: String) -> Self {
        Prompt {
            parts: vec![PromptPart::Text(s)],
        }
    }
}

impl From<&str> for Prompt {
    fn from(s: &str) -> Self {
        Prompt {
            parts: vec![PromptPart::Text(s.to_string())],
        }
    }
}

pub type ChunkId = String;

#[derive(Clone, Debug)]
pub enum TokenizerChunk {
    Text(Vec<LlamaToken>, ChunkId),
    Image(Arc<MtmdInputChunks>, ChunkId),
}

impl TokenizerChunk {
    pub fn new_text(tokens: Vec<LlamaToken>) -> Self {
        let mut hasher = AHasher::default();
        tokens.hash(&mut hasher);
        let id = hasher.finish().to_string();
        Self::Text(tokens, id)
    }

    pub fn new_image(chunks: MtmdInputChunks) -> Self {
        let id = (0..chunks.len()).find_map(|i| {
            chunks
                .get(i)
                .filter(|c| c.chunk_type() == MtmdInputChunkType::Image)
                .map(|c| c.id().unwrap_or_default())
        });

        // We use unwrap or default here, as everything should always exist
        // & returning Result here would be the opposite of ergonomical
        Self::Image(Arc::new(chunks), id.unwrap_or_default())
    }

    pub fn id(&self) -> ChunkId {
        match self {
            Self::Text(_, id) | Self::Image(_, id) => id.clone(),
        }
    }

    pub fn n_tokens(&self) -> usize {
        match self {
            TokenizerChunk::Text(tokens, _) => tokens.len(),
            TokenizerChunk::Image(chunks_arc, _) => (0..chunks_arc.len())
                .map(|i| chunks_arc.get(i).map(|c| c.n_tokens()).unwrap_or(0))
                .sum(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct TokenizerChunks {
    chunks: Vec<TokenizerChunk>,
}

impl TokenizerChunks {
    pub fn n_tokens(&self) -> usize {
        self.chunks.iter().map(|chunk| chunk.n_tokens()).sum()
    }

    pub fn len(&self) -> usize {
        self.chunks.len()
    }

    pub fn new() -> Self {
        Self { chunks: vec![] }
    }

    pub fn iter(&self) -> impl Iterator<Item = &TokenizerChunk> {
        self.chunks.iter()
    }

    pub fn into_iter(self) -> impl Iterator<Item = TokenizerChunk> {
        self.chunks.into_iter()
    }

    pub fn get(&self, index: usize) -> &TokenizerChunk {
        &self.chunks[index]
    }

    pub fn list_ids(&self) -> Vec<String> {
        self.chunks.iter().map(|chunk| chunk.id()).collect()
    }

    pub fn append(&mut self, other: TokenizerChunk) -> &mut Self {
        let next = match (self.chunks.pop(), other) {
            (Some(TokenizerChunk::Text(tokens, _)), TokenizerChunk::Text(other_tokens, _)) => {
                let tokens = tokens
                    .into_iter()
                    .chain(other_tokens.into_iter())
                    .collect::<Vec<_>>();

                TokenizerChunk::new_text(tokens)
            }
            (Some(last), other) => {
                self.chunks.push(last);
                other
            }
            (_, other) => other,
        };

        self.chunks.push(next);
        self
    }

    /// Returns [start, end) position of the chunk at the given index.
    pub fn chunk_bounds(&self, index: usize) -> (usize, usize) {
        let mut start = 0;
        let mut i = 0;

        while i < index {
            start += self.chunks[i].n_tokens();
            i += 1;
        }

        let end = start + self.chunks[i].n_tokens();
        (start, end)
    }

    pub fn tail(&self, from_pos: usize) -> TokenizerChunks {
        if from_pos >= self.n_tokens() {
            return TokenizerChunks::new();
        }

        // Find the chunk that contains from_pos
        let mut pos = 0;
        let mut i = 0;
        while i < self.chunks.len() {
            let chunk_size = self.chunks[i].n_tokens();
            if pos + chunk_size > from_pos {
                // from_pos is within this chunk
                break;
            }
            pos += chunk_size;
            i += 1;
        }

        // Calculate offset within the current chunk
        let offset_in_chunk = from_pos - pos;

        match &self.chunks[i] {
            TokenizerChunk::Text(tokens, _) => {
                let (_, tail_tokens) = tokens.split_at(offset_in_chunk);
                let mut new_chunks = vec![TokenizerChunk::new_text(tail_tokens.to_vec())];
                new_chunks.extend_from_slice(&self.chunks[i + 1..]);

                TokenizerChunks { chunks: new_chunks }
            }
            TokenizerChunk::Image(_chunks, _) => TokenizerChunks {
                chunks: self.chunks[i..].to_vec(),
            },
        }
    }
}

pub fn find_chunks_prefix_difference(
    old: &TokenizerChunks,
    new: &TokenizerChunks,
) -> (usize, TokenizerChunks) {
    let longest_common_chunk_prefix_index = new
        .iter()
        .zip(old.iter())
        .position(|(a, b)| a.id() != b.id());

    // common prefix found, we just need to find if the new is longer or shorter than the old
    let Some(chunk_prefix_index) = longest_common_chunk_prefix_index else {
        if old.len() >= new.len() {
            return (new.n_tokens(), new.tail(new.n_tokens()));
        } else {
            return (old.n_tokens(), new.tail(old.n_tokens()));
        }
    };
    let (new_start, _) = new.chunk_bounds(chunk_prefix_index);

    // text and text are colliding, we are going into the tokens
    if let (TokenizerChunk::Text(new_tokens, _), TokenizerChunk::Text(old_tokens, _)) =
        (new.get(chunk_prefix_index), old.get(chunk_prefix_index))
    {
        let longest_common_prefix_index = new_tokens
            .iter()
            .zip(old_tokens.iter())
            .position(|(a, b)| a != b);

        if let Some(token_prefix_index) = longest_common_prefix_index {
            return (
                new_start + token_prefix_index,
                new.tail(new_start + token_prefix_index),
            );
        }
    }

    // image and image, or image and text are colliding
    return (new_start, new.tail(new_start));
}

// Here, the model is represented implicitly by the MTMD context
#[derive(Debug)]
pub struct ProjectionModel {
    pub ctx: MtmdContext, // TODO: Make models abstraction layer (projection model, main model, etc.) and force encapsulation
}

impl ProjectionModel {
    pub fn from_path(path: &str, parent_model: &LlamaModel) -> Result<Self, MultimodalError> {
        let n_threads = std::thread::available_parallelism()
            .map(|p| p.get() as i32)
            .unwrap_or(4);

        let media_marker = llama_cpp_2::mtmd::mtmd_default_marker().to_string();

        let mtmd_params = MtmdContextParams {
            use_gpu: has_discrete_gpu(),
            print_timings: false,
            n_threads,
            media_marker: CString::new(media_marker.to_string())
                .expect("Failed to create CString for marker"),
        };

        match MtmdContext::init_from_file(path, parent_model, &mtmd_params) {
            Ok(ctx) => {
                info!("MTMD context initialized successfully");
                Ok(Self { ctx })
            }
            Err(e) => {
                warn!(error = %e, "Failed to initialize MTMD context:");

                Err(MultimodalError::ContextNotInitialized)
            }
        }
    }

    pub fn tokenize(&self, bitmap: &MtmdBitmap) -> Result<TokenizerChunk, TokenizationError> {
        let media_marker = llama_cpp_2::mtmd::mtmd_default_marker().to_string();
        let mtmd_chunks = self
            .ctx
            .tokenize(
                MtmdInputText {
                    text: media_marker,
                    add_special: false,
                    parse_special: true,
                },
                &[bitmap],
            )
            .map_err(|e| TokenizationError::ProjectionTokenizationError(e.to_string()))?;

        Ok(TokenizerChunk::new_image(mtmd_chunks))
    }

    pub fn load_image(&self, path: &str) -> Result<MtmdBitmap, MultimodalError> {
        let bitmap =
            MtmdBitmap::from_file(&self.ctx, &path).map_err(|e| MultimodalError::LoadImage {
                path: path.to_string(),
                error: e.to_string(),
            })?;
        info!(path = %path, "Loaded image for MTMD");

        Ok(bitmap)
    }
}

#[derive(Debug)]
pub struct Tokenizer<'a> {
    model: &'a Arc<LlamaModel>,
    projection_model: Option<Arc<ProjectionModel>>,
    add_bos: AddBos,
}

impl<'a> Tokenizer<'a> {
    pub fn new(
        model: &'a Arc<LlamaModel>,
        projection_model: Option<Arc<ProjectionModel>>,
        add_bos: AddBos,
    ) -> Self {
        Self {
            projection_model,
            add_bos,
            model,
        }
    }

    pub fn tokenize(
        &self,
        rendered_chat: String,
        bitmaps: Vec<&MtmdBitmap>,
    ) -> Result<TokenizerChunks, TokenizationError> {
        let text_chunks = self.tokenize_text(&rendered_chat)?;

        let n_image_markers = text_chunks.len() - 1;
        if n_image_markers != bitmaps.len() {
            let preview = rendered_chat.chars().take(200).collect::<String>();
            return Err(TokenizationError::ImageMarkerMismatch {
                n_markers: n_image_markers,
                n_bitmaps: bitmaps.len(),
                template_preview: preview,
            });
        }

        let image_chunks = self.tokenize_images(bitmaps)?;
        let chunks = self
            .interleave(text_chunks, image_chunks)
            .into_iter()
            .filter(|chunk| chunk.n_tokens() > 0)
            .collect();

        Ok(TokenizerChunks { chunks })
    }

    fn tokenize_text(&self, text: &str) -> Result<Vec<TokenizerChunk>, TokenizationError> {
        let media_marker = llama_cpp_2::mtmd::mtmd_default_marker().to_string();
        let splits = text
            .split(media_marker.as_str())
            .enumerate()
            .map(|(idx, split)| {
                self.model
                    .str_to_token(split, AddBos::Never)
                    .map(|tokens| TokenizerChunk::new_text(tokens))
                    .map_err(|e| TokenizationError::TextTokenizationFailed {
                        position: idx,
                        text_preview: split.chars().take(100).collect(),
                        error: e.to_string(),
                    })
            })
            .collect::<Result<Vec<TokenizerChunk>, TokenizationError>>()?;

        Ok(splits)
    }

    fn tokenize_images(
        &self,
        bitmaps: Vec<&MtmdBitmap>,
    ) -> Result<Vec<TokenizerChunk>, TokenizationError> {
        let projection_model = self.projection_model.as_ref().ok_or(
            TokenizationError::ProjectionTokenizationError("Context not initialized".to_string()),
        )?;

        // Tokenize each image separately to get individual chunks
        bitmaps
            .iter()
            .map(|bitmap| projection_model.tokenize(*bitmap))
            .collect::<Result<Vec<_>, TokenizationError>>()
    }

    fn interleave<T>(&self, v1: Vec<T>, v2: Vec<T>) -> Vec<T> {
        let mut ai = v1.into_iter();
        let mut bi = v2.into_iter();
        let mut out = Vec::new();

        loop {
            match (ai.next(), bi.next()) {
                (Some(x), Some(y)) => {
                    out.push(x);
                    out.push(y);
                }
                (Some(x), None) => {
                    out.push(x);
                    out.extend(ai);
                    break;
                }
                (None, Some(y)) => {
                    out.push(y);
                    out.extend(bi);
                    break;
                }
                (None, None) => break,
            }
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use llama_cpp_2::mtmd::MtmdInputChunks;

    // Test helper functions to create chunks without needing full model/MTMD context

    /// Creates a text chunk with the given token IDs
    fn create_text_chunk(tokens: Vec<i32>) -> TokenizerChunk {
        let llama_tokens: Vec<LlamaToken> = tokens.into_iter().map(LlamaToken::new).collect();
        TokenizerChunk::new_text(llama_tokens)
    }

    /// Creates TokenizerChunks from a vector of chunks
    fn create_chunks(chunks: Vec<TokenizerChunk>) -> TokenizerChunks {
        TokenizerChunks { chunks }
    }

    /// Creates a mock image chunk with a given ID
    /// Since we can't easily create real MtmdInputChunks without a model,
    /// we'll use the internal constructor directly for testing
    fn create_image_chunk(id: &str) -> TokenizerChunk {
        // Create an empty MtmdInputChunks as a placeholder
        let chunks = MtmdInputChunks::new();
        // Manually construct with an ID for testing purposes
        TokenizerChunk::Image(Arc::new(chunks), id.to_string())
    }

    // ===== A. Text-Only Tests =====

    #[test]
    fn test_text_only_identical() {
        // Old: ["Hello", " world"]
        // New: ["Hello", " world"]
        // Expected: prefix_index = total tokens, tail = empty
        let old = create_chunks(vec![
            create_text_chunk(vec![1, 2, 3]), // "Hello"
            create_text_chunk(vec![4, 5, 6]), // " world"
        ]);
        let new = create_chunks(vec![
            create_text_chunk(vec![1, 2, 3]), // "Hello"
            create_text_chunk(vec![4, 5, 6]), // " world"
        ]);

        let (prefix_index, tail) = find_chunks_prefix_difference(&old, &new);

        assert_eq!(prefix_index, 6); // All 6 tokens match
        assert_eq!(tail.n_tokens(), 0); // Nothing to reload
    }

    #[test]
    fn test_text_only_new_longer() {
        // Old: ["Hello"]
        // New: ["Hello", " world"]
        // Expected: prefix_index = tokens in "Hello", tail = tokens in " world"
        let old = create_chunks(vec![create_text_chunk(vec![1, 2, 3])]);
        let new = create_chunks(vec![
            create_text_chunk(vec![1, 2, 3]), // "Hello"
            create_text_chunk(vec![4, 5, 6]), // " world"
        ]);

        let (prefix_index, tail) = find_chunks_prefix_difference(&old, &new);

        assert_eq!(prefix_index, 3); // First 3 tokens match
        assert_eq!(tail.n_tokens(), 3); // Need to load " world"
    }

    #[test]
    fn test_text_only_new_shorter() {
        // Old: ["Hello", " world"]
        // New: ["Hello"]
        // Expected: prefix_index = tokens in "Hello", tail = empty
        let old = create_chunks(vec![
            create_text_chunk(vec![1, 2, 3]), // "Hello"
            create_text_chunk(vec![4, 5, 6]), // " world"
        ]);
        let new = create_chunks(vec![create_text_chunk(vec![1, 2, 3])]);

        let (prefix_index, tail) = find_chunks_prefix_difference(&old, &new);

        assert_eq!(prefix_index, 3); // First 3 tokens match
        assert_eq!(tail.n_tokens(), 0); // Nothing to reload
    }

    #[test]
    fn test_text_only_partial_overlap_in_chunk() {
        // Old: ["Hello world"] (single chunk, tokens [1,2,3,4,5])
        // New: ["Hello there"] (single chunk, tokens [1,2,3,6,7])
        // Expected: prefix_index = tokens in "Hello ", tail = tokens in "there"
        // Tests token-level diffing within a text chunk
        let old = create_chunks(vec![create_text_chunk(vec![1, 2, 3, 4, 5])]);
        let new = create_chunks(vec![create_text_chunk(vec![1, 2, 3, 6, 7])]);

        let (prefix_index, tail) = find_chunks_prefix_difference(&old, &new);

        assert_eq!(prefix_index, 3); // First 3 tokens match
        assert_eq!(tail.n_tokens(), 2); // Need to load tokens [6, 7]
    }

    #[test]
    fn test_text_only_no_overlap() {
        // Old: ["Hello"]
        // New: ["Goodbye"]
        // Expected: prefix_index = 0, tail = all new tokens
        let old = create_chunks(vec![create_text_chunk(vec![1, 2, 3])]);
        let new = create_chunks(vec![create_text_chunk(vec![4, 5, 6])]);

        let (prefix_index, tail) = find_chunks_prefix_difference(&old, &new);

        assert_eq!(prefix_index, 0); // No common prefix
        assert_eq!(tail.n_tokens(), 3); // All new tokens
    }

    #[test]
    fn test_text_only_empty_old() {
        // Old: [] (empty)
        // New: ["Hello"]
        // Expected: prefix_index = 0, tail = all new tokens
        let old = create_chunks(vec![]);
        let new = create_chunks(vec![create_text_chunk(vec![1, 2, 3])]);

        let (prefix_index, tail) = find_chunks_prefix_difference(&old, &new);

        assert_eq!(prefix_index, 0); // No common prefix
        assert_eq!(tail.n_tokens(), 3); // All new tokens
    }

    #[test]
    fn test_text_only_empty_new() {
        // Old: ["Hello"]
        // New: [] (empty)
        // Expected: prefix_index = 0, tail = empty
        let old = create_chunks(vec![create_text_chunk(vec![1, 2, 3])]);
        let new = create_chunks(vec![]);

        let (prefix_index, tail) = find_chunks_prefix_difference(&old, &new);

        assert_eq!(prefix_index, 0); // No common prefix
        assert_eq!(tail.n_tokens(), 0); // Nothing to reload
    }

    #[test]
    fn test_text_only_multiple_chunks_differ_at_boundary() {
        // Old: ["Hello", " ", "world"]
        // New: ["Hello", " ", "there"]
        // Expected: Differs at chunk 2, returns appropriate prefix_index and tail
        let old = create_chunks(vec![
            create_text_chunk(vec![1, 2, 3]),    // "Hello"
            create_text_chunk(vec![4]),          // " "
            create_text_chunk(vec![5, 6, 7, 8]), // "world"
        ]);
        let new = create_chunks(vec![
            create_text_chunk(vec![1, 2, 3]),   // "Hello"
            create_text_chunk(vec![4]),         // " "
            create_text_chunk(vec![9, 10, 11]), // "there"
        ]);

        let (prefix_index, tail) = find_chunks_prefix_difference(&old, &new);

        assert_eq!(prefix_index, 4); // "Hello" (3) + " " (1) = 4 tokens match
        assert_eq!(tail.n_tokens(), 3); // Need to load "there"
    }

    // ===== B. Image Tests =====

    #[test]
    fn test_image_only_identical() {
        // Old: [Image(chunks_a)] (same image ID)
        // New: [Image(chunks_a)] (same image ID)
        // Expected: prefix_index = image token count, tail = empty
        let old = create_chunks(vec![create_image_chunk("image_1")]);
        let new = create_chunks(vec![create_image_chunk("image_1")]);

        let (prefix_index, tail) = find_chunks_prefix_difference(&old, &new);

        assert_eq!(prefix_index, 0); // Images match (0 tokens in empty MtmdInputChunks)
        assert_eq!(tail.n_tokens(), 0); // Nothing to reload
    }

    #[test]
    fn test_image_only_collision() {
        // Old: [Image(chunks_a)]
        // New: [Image(chunks_b)] (different image ID)
        // Expected: prefix_index = 0 (start of first chunk), tail = Image(chunks_b) entirely
        // Validates that image collisions cause full image reload
        let old = create_chunks(vec![create_image_chunk("image_1")]);
        let new = create_chunks(vec![create_image_chunk("image_2")]);

        let (prefix_index, tail) = find_chunks_prefix_difference(&old, &new);

        assert_eq!(prefix_index, 0); // Images differ, restart from beginning
        assert_eq!(tail.n_tokens(), 0); // Empty MtmdInputChunks has 0 tokens
    }

    #[test]
    fn test_image_new_longer() {
        // Old: [Image(img1)]
        // New: [Image(img1), Image(img2)]
        // Expected: prefix_index = tokens in img1, tail = Image(img2)
        let old = create_chunks(vec![create_image_chunk("image_1")]);
        let new = create_chunks(vec![
            create_image_chunk("image_1"),
            create_image_chunk("image_2"),
        ]);

        let (prefix_index, tail) = find_chunks_prefix_difference(&old, &new);

        assert_eq!(prefix_index, 0); // img1 matches (0 tokens)
        assert_eq!(tail.n_tokens(), 0); // img2 to load (0 tokens in empty chunk)
    }

    #[test]
    fn test_image_new_shorter() {
        // Old: [Image(img1), Image(img2)]
        // New: [Image(img1)]
        // Expected: prefix_index = tokens in img1, tail = empty
        let old = create_chunks(vec![
            create_image_chunk("image_1"),
            create_image_chunk("image_2"),
        ]);
        let new = create_chunks(vec![create_image_chunk("image_1")]);

        let (prefix_index, tail) = find_chunks_prefix_difference(&old, &new);

        assert_eq!(prefix_index, 0); // img1 matches (0 tokens)
        assert_eq!(tail.n_tokens(), 0); // Nothing to reload
    }

    // ===== C. Mixed Text and Image Tests =====

    #[test]
    fn test_mixed_text_then_image_identical() {
        // Old: [Text("Hello"), Image(img1)]
        // New: [Text("Hello"), Image(img1)]
        // Expected: prefix_index = total tokens, tail = empty
        let old = create_chunks(vec![
            create_text_chunk(vec![1, 2, 3]),
            create_image_chunk("image_1"),
        ]);
        let new = create_chunks(vec![
            create_text_chunk(vec![1, 2, 3]),
            create_image_chunk("image_1"),
        ]);

        let (prefix_index, tail) = find_chunks_prefix_difference(&old, &new);

        assert_eq!(prefix_index, 3); // All tokens match
        assert_eq!(tail.n_tokens(), 0); // Nothing to reload
    }

    #[test]
    fn test_mixed_text_then_image_image_collision() {
        // Old: [Text("Hello"), Image(img1)]
        // New: [Text("Hello"), Image(img2)]
        // Expected: prefix_index = tokens in "Hello", tail = Image(img2)
        // Validates that text prefix is preserved, but image is reloaded
        let old = create_chunks(vec![
            create_text_chunk(vec![1, 2, 3]),
            create_image_chunk("image_1"),
        ]);
        let new = create_chunks(vec![
            create_text_chunk(vec![1, 2, 3]),
            create_image_chunk("image_2"),
        ]);

        let (prefix_index, tail) = find_chunks_prefix_difference(&old, &new);

        assert_eq!(prefix_index, 3); // Text matches
        assert_eq!(tail.n_tokens(), 0); // Image to reload (0 tokens in empty chunk)
    }

    #[test]
    fn test_mixed_text_collision_before_image() {
        // Old: [Text("Hello"), Image(img1)]
        // New: [Text("Goodbye"), Image(img1)]
        // Expected: prefix_index = 0, tail = entire new chunks
        // Text differs first, so everything reloads
        let old = create_chunks(vec![
            create_text_chunk(vec![1, 2, 3]),
            create_image_chunk("image_1"),
        ]);
        let new = create_chunks(vec![
            create_text_chunk(vec![4, 5, 6]),
            create_image_chunk("image_1"),
        ]);

        let (prefix_index, tail) = find_chunks_prefix_difference(&old, &new);

        assert_eq!(prefix_index, 0); // Text differs immediately
        assert_eq!(tail.n_tokens(), 3); // All new tokens (text only, image has 0)
    }

    #[test]
    fn test_mixed_text_partial_collision_before_image() {
        // Old: [Text("Hello world"), Image(img1)]
        // New: [Text("Hello there"), Image(img1)]
        // Expected: prefix_index = tokens in "Hello ", tail = tokens in "there" + Image(img1)
        // Token-level diffing in text chunk, then image reloaded
        let old = create_chunks(vec![
            create_text_chunk(vec![1, 2, 3, 4, 5]), // "Hello world"
            create_image_chunk("image_1"),
        ]);
        let new = create_chunks(vec![
            create_text_chunk(vec![1, 2, 3, 6, 7]), // "Hello there"
            create_image_chunk("image_1"),
        ]);

        let (prefix_index, tail) = find_chunks_prefix_difference(&old, &new);

        assert_eq!(prefix_index, 3); // First 3 tokens match
        assert_eq!(tail.n_tokens(), 2); // tokens [6, 7] (image part gets cut off in tail)
    }

    #[test]
    fn test_mixed_image_then_text_identical() {
        // Old: [Image(img1), Text("Hello")]
        // New: [Image(img1), Text("Hello")]
        // Expected: prefix_index = total tokens, tail = empty
        let old = create_chunks(vec![
            create_image_chunk("image_1"),
            create_text_chunk(vec![1, 2, 3]),
        ]);
        let new = create_chunks(vec![
            create_image_chunk("image_1"),
            create_text_chunk(vec![1, 2, 3]),
        ]);

        let (prefix_index, tail) = find_chunks_prefix_difference(&old, &new);

        assert_eq!(prefix_index, 3); // All tokens match (0 from image + 3 from text)
        assert_eq!(tail.n_tokens(), 0); // Nothing to reload
    }

    #[test]
    fn test_mixed_image_then_text_text_differs() {
        // Old: [Image(img1), Text("Hello")]
        // New: [Image(img1), Text("Goodbye")]
        // Expected: prefix_index = tokens in img1, tail = Text("Goodbye")
        let old = create_chunks(vec![
            create_image_chunk("image_1"),
            create_text_chunk(vec![1, 2, 3]),
        ]);
        let new = create_chunks(vec![
            create_image_chunk("image_1"),
            create_text_chunk(vec![4, 5, 6]),
        ]);

        let (prefix_index, tail) = find_chunks_prefix_difference(&old, &new);

        assert_eq!(prefix_index, 0); // Image matches (0 tokens)
        assert_eq!(tail.n_tokens(), 3); // New text tokens
    }

    #[test]
    fn test_mixed_complex_interleaving() {
        // Old: [Text("A"), Image(img1), Text("B"), Image(img2)]
        // New: [Text("A"), Image(img1), Text("B"), Image(img3)]
        // Expected: Differs at img2 vs img3, appropriate prefix and tail
        let old = create_chunks(vec![
            create_text_chunk(vec![1]),
            create_image_chunk("image_1"),
            create_text_chunk(vec![2]),
            create_image_chunk("image_2"),
        ]);
        let new = create_chunks(vec![
            create_text_chunk(vec![1]),
            create_image_chunk("image_1"),
            create_text_chunk(vec![2]),
            create_image_chunk("image_3"),
        ]);

        let (prefix_index, tail) = find_chunks_prefix_difference(&old, &new);

        assert_eq!(prefix_index, 2); // Text("A") + Image(img1) + Text("B") = 2 tokens
        assert_eq!(tail.n_tokens(), 0); // Image(img3) to reload (0 tokens)
    }

    #[test]
    fn test_mixed_text_to_image_collision() {
        // Old: [Text("Hello"), Text("world")]
        // New: [Text("Hello"), Image(img1)]
        // Expected: Chunks differ at position 1 (Text vs Image), returns appropriate result
        // Type collision between Text and Image chunks
        let old = create_chunks(vec![
            create_text_chunk(vec![1, 2, 3]),
            create_text_chunk(vec![4, 5, 6]),
        ]);
        let new = create_chunks(vec![
            create_text_chunk(vec![1, 2, 3]),
            create_image_chunk("image_1"),
        ]);

        let (prefix_index, tail) = find_chunks_prefix_difference(&old, &new);

        assert_eq!(prefix_index, 3); // First text chunk matches
        assert_eq!(tail.n_tokens(), 0); // Image to reload (0 tokens)
    }

    #[test]
    fn test_mixed_image_to_text_collision() {
        // Old: [Image(img1), Text("world")]
        // New: [Text("Hello"), Text("world")]
        // Expected: Chunks differ at position 0, full reload
        let old = create_chunks(vec![
            create_image_chunk("image_1"),
            create_text_chunk(vec![4, 5, 6]),
        ]);
        let new = create_chunks(vec![
            create_text_chunk(vec![1, 2, 3]),
            create_text_chunk(vec![4, 5, 6]),
        ]);

        let (prefix_index, tail) = find_chunks_prefix_difference(&old, &new);

        assert_eq!(prefix_index, 0); // Type collision at position 0
        assert_eq!(tail.n_tokens(), 6); // All new tokens
    }

    // ===== D. Edge Cases =====

    #[test]
    fn test_empty_both() {
        // Old: []
        // New: []
        // Expected: prefix_index = 0, tail = empty
        let old = create_chunks(vec![]);
        let new = create_chunks(vec![]);

        let (prefix_index, tail) = find_chunks_prefix_difference(&old, &new);

        assert_eq!(prefix_index, 0);
        assert_eq!(tail.n_tokens(), 0);
    }

    #[test]
    fn test_very_long_common_prefix() {
        // Old: 100 text chunks followed by one different chunk
        // New: Same 100 text chunks followed by a different chunk
        // Expected: Validates efficiency with long common prefixes
        let mut old_chunks = Vec::new();
        let mut new_chunks = Vec::new();

        // Create 100 identical chunks
        for i in 0..100 {
            old_chunks.push(create_text_chunk(vec![i, i + 1, i + 2]));
            new_chunks.push(create_text_chunk(vec![i, i + 1, i + 2]));
        }

        // Add different final chunks
        old_chunks.push(create_text_chunk(vec![1000, 1001]));
        new_chunks.push(create_text_chunk(vec![2000, 2001]));

        let old = create_chunks(old_chunks);
        let new = create_chunks(new_chunks);

        let (prefix_index, tail) = find_chunks_prefix_difference(&old, &new);

        assert_eq!(prefix_index, 300); // 100 chunks * 3 tokens each
        assert_eq!(tail.n_tokens(), 2); // Final different chunk
    }
}
