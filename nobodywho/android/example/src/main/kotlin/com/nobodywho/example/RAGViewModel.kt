package com.nobodywho.example

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.nobodywho.Chat
import com.nobodywho.HybridSemanticMemory
import com.nobodywho.LlamaCppEmbedder
import com.nobodywho.LlamaCppReranker
import com.nobodywho.NobodyWhoModel
import com.nobodywho.PromptTemplate
import com.nobodywho.SQLiteVectorStore
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

data class RAGMessage(
    val text: String,
    val isUser: Boolean,
    val sources: List<String> = emptyList()
)

class RAGViewModel(application: Application) : AndroidViewModel(application) {

    private val _messages = MutableStateFlow<List<RAGMessage>>(emptyList())
    val messages: StateFlow<List<RAGMessage>> = _messages.asStateFlow()

    private val _isReady = MutableStateFlow(false)
    val isReady: StateFlow<Boolean> = _isReady.asStateFlow()

    private val _isInitializing = MutableStateFlow(false)
    val isInitializing: StateFlow<Boolean> = _isInitializing.asStateFlow()

    private val _isProcessing = MutableStateFlow(false)
    val isProcessing: StateFlow<Boolean> = _isProcessing.asStateFlow()

    private val _isIndexing = MutableStateFlow(false)
    val isIndexing: StateFlow<Boolean> = _isIndexing.asStateFlow()

    private val _indexingStatus = MutableStateFlow("")
    val indexingStatus: StateFlow<String> = _indexingStatus.asStateFlow()

    private val _documentCount = MutableStateFlow(0)
    val documentCount: StateFlow<Int> = _documentCount.asStateFlow()

    private var chat: Chat? = null
    private var hybridMemory: HybridSemanticMemory? = null

    fun initialize(
        chatModel: NobodyWhoModel,
        embedderModelPath: String,
        rerankerModelPath: String? = null,
        useGpu: Boolean = true
    ) {
        if (_isReady.value || _isInitializing.value) return
        viewModelScope.launch {
            _isInitializing.value = true
            try {
                val (c, m) = withContext(Dispatchers.IO) {
                    // Reuse the already-loaded model weights — no second file load.
                    val c = Chat(model = chatModel)

                    val embedder = LlamaCppEmbedder(
                        modelPath = embedderModelPath,
                        useGpu = useGpu,
                        contextSize = 2048
                    )

                    val reranker = rerankerModelPath?.let {
                        LlamaCppReranker(modelPath = it, useGpu = useGpu, contextSize = 4096)
                    }

                    val vectorStore = SQLiteVectorStore(
                        context = getApplication(),
                        databaseName = "rag_vectors.db"
                    )

                    val m = HybridSemanticMemory(
                        embedder = embedder,
                        vectorStore = vectorStore,
                        reranker = reranker
                    )

                    c to m
                }

                chat = c
                hybridMemory = m
                _documentCount.value = m.documentCount
                _isReady.value = true

                if (m.documentCount == 0) {
                    indexSampleDocument()
                }

            } catch (e: Exception) {
                _messages.value = _messages.value + RAGMessage(
                    text = "Initialization failed: ${e.message}",
                    isUser = false
                )
            } finally {
                _isInitializing.value = false
            }
        }
    }

    fun indexText(id: String, text: String, metadata: Map<String, String> = emptyMap()) {
        val memory = hybridMemory ?: return
        viewModelScope.launch {
            _isIndexing.value = true
            try {
                val chunks = chunkText(text, chunkSize = 500, overlap = 50)
                var indexed = 0
                chunks.forEachIndexed { i, chunk ->
                    _indexingStatus.value = "Embedding ${i + 1} / ${chunks.size}..."
                    withContext(Dispatchers.IO) {
                        try {
                            memory.saveDocument(
                                id = "${id}_$i",
                                text = chunk,
                                metadata = metadata + mapOf("chunk" to "$i")
                            )
                            indexed++
                        } catch (e: Exception) {
                            // Skip chunks that fail to embed.
                        }
                    }
                }
                _documentCount.value = memory.documentCount
                _messages.value = _messages.value + RAGMessage(
                    text = "Indexed $indexed/${chunks.size} chunks.",
                    isUser = false
                )
            } finally {
                _isIndexing.value = false
                _indexingStatus.value = ""
            }
        }
    }

    fun sendMessage(query: String, topK: Int = 3, useReranker: Boolean = false) {
        val chat = chat ?: return
        val memory = hybridMemory ?: return
        if (query.isBlank()) return

        _messages.value = _messages.value + RAGMessage(text = query, isUser = true)
        _isProcessing.value = true

        viewModelScope.launch {
            try {
                val (answer, sources) = withContext(Dispatchers.IO) {
                    val results = memory.search(
                        query = query,
                        topK = topK,
                        rerankCandidates = if (useReranker) 10 else topK
                    )
                    if (results.isEmpty()) {
                        return@withContext "No relevant documents found. Add some documents first." to emptyList()
                    }
                    val prompt = PromptTemplate.default.build(query = query, documents = results)
                    val answer = chat.ask(prompt)
                    answer to results.map { it.metadata["source"] ?: it.text.take(40) }
                }

                _messages.value = _messages.value + RAGMessage(
                    text = answer,
                    isUser = false,
                    sources = sources
                )
            } catch (e: Exception) {
                _messages.value = _messages.value + RAGMessage(
                    text = "Error: ${e.message}",
                    isUser = false
                )
            } finally {
                _isProcessing.value = false
            }
        }
    }

    fun clearDocuments() {
        hybridMemory?.clear()
        _documentCount.value = 0
        _messages.value = _messages.value + RAGMessage(
            text = "Cleared all documents.",
            isUser = false
        )
    }

    private suspend fun indexSampleDocument() {
        val sample = """
            NobodyWho is an on-device AI framework for Android. It provides local language model
            inference using llama.cpp, meaning all computation runs privately on your device.

            Retrieval-Augmented Generation (RAG) combines a language model with a knowledge base.
            When you ask a question, the system searches the knowledge base for relevant passages,
            then provides those passages as context to the language model.

            The NobodyWho RAG system consists of: a chat model that generates responses, an
            embedder that converts text into vectors for semantic search, a vector store (SQLite)
            that stores and retrieves vectors, and an optional reranker that improves results.

            Embeddings are dense vectors that capture the semantic meaning of text. Similar texts
            produce vectors that are close together in vector space, even with different words.

            Chunking splits long documents into smaller pieces before embedding. Each chunk should
            be short enough to fit within the model context but long enough to be meaningful.

            Privacy is a core advantage of on-device AI. Models run locally, so your documents
            and queries never leave your device.
        """.trimIndent()

        indexText(id = "sample", text = sample, metadata = mapOf("source" to "sample_doc.txt"))
    }

    // MARK: - Text Chunking

    private fun chunkText(text: String, chunkSize: Int, overlap: Int): List<String> {
        val sentences = text.split(Regex("(?<=[.!?])\\s+")).filter { it.isNotBlank() }
        val chunks = mutableListOf<String>()
        var window = mutableListOf<String>()
        var windowLen = 0

        for (sentence in sentences) {
            if (sentence.length > chunkSize) {
                if (window.isNotEmpty()) {
                    chunks.add(window.joinToString(" "))
                    window = mutableListOf()
                    windowLen = 0
                }
                chunks.addAll(splitByWords(sentence, chunkSize))
                continue
            }

            if (windowLen + sentence.length > chunkSize && window.isNotEmpty()) {
                chunks.add(window.joinToString(" "))
                val seed = mutableListOf<String>()
                var seedLen = 0
                for (s in window.reversed()) {
                    if (seedLen + s.length + 1 > overlap) break
                    seed.add(0, s)
                    seedLen += s.length + 1
                }
                window = seed
                windowLen = seedLen
            }

            window.add(sentence)
            windowLen += sentence.length + 1
        }

        if (window.isNotEmpty()) chunks.add(window.joinToString(" "))
        return chunks
    }

    private fun splitByWords(text: String, chunkSize: Int): List<String> {
        val words = text.split(Regex("\\s+"))
        val chunks = mutableListOf<String>()
        var current = StringBuilder()

        for (word in words) {
            if (current.length + word.length + 1 > chunkSize && current.isNotEmpty()) {
                chunks.add(current.toString())
                current = StringBuilder(word)
            } else {
                if (current.isNotEmpty()) current.append(' ')
                current.append(word)
            }
        }
        if (current.isNotEmpty()) chunks.add(current.toString())
        return chunks
    }
}
