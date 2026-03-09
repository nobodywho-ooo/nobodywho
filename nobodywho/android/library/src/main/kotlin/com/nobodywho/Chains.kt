package com.nobodywho

// MARK: - Chain

/** A composable retrieval or retrieval-and-inference pipeline. */
interface Chain {
    suspend fun execute(query: String): String
}

/** Retrieves relevant documents and returns them as a formatted string. */
class RetrievalChain(
    private val semanticMemory: SemanticMemory,
    private val topK: Int = 3
) : Chain {
    override suspend fun execute(query: String): String {
        val documents = semanticMemory.search(query = query, topK = topK)
        return documents.joinToString("\n\n") { "[${it.score}] ${it.text}" }
    }
}

/** Retrieves relevant documents and generates an answer with a language model. */
class RetrievalAndInferenceChain(
    private val semanticMemory: SemanticMemory,
    private val languageModel: LanguageModel,
    private val topK: Int = 3,
    private val promptTemplate: PromptTemplate = PromptTemplate.default
) : Chain {
    override suspend fun execute(query: String): String {
        val documents = semanticMemory.search(query = query, topK = topK)
        val prompt = promptTemplate.build(query = query, documents = documents)
        return languageModel.generate(prompt)
    }

    /** Executes the chain and returns both the generated answer and the retrieved documents. */
    suspend fun executeWithContext(query: String): Pair<String, List<ScoredDocument>> {
        val documents = semanticMemory.search(query = query, topK = topK)
        val prompt = promptTemplate.build(query = query, documents = documents)
        val answer = languageModel.generate(prompt)
        return answer to documents
    }
}

/** Retrieves with two-stage hybrid search (embedding + reranking) and generates an answer. */
class HybridRetrievalAndInferenceChain(
    private val hybridMemory: HybridSemanticMemory,
    private val languageModel: LanguageModel,
    private val topK: Int = 3,
    private val rerankCandidates: Int = 10,
    private val promptTemplate: PromptTemplate = PromptTemplate.default
) : Chain {
    override suspend fun execute(query: String): String {
        val documents = hybridMemory.search(
            query = query,
            topK = topK,
            rerankCandidates = rerankCandidates
        )
        val prompt = promptTemplate.build(query = query, documents = documents)
        return languageModel.generate(prompt)
    }
}

// MARK: - Prompt Templates

/** Builds a prompt from a query and retrieved documents. */
class PromptTemplate(private val builder: (String, List<ScoredDocument>) -> String) {

    fun build(query: String, documents: List<ScoredDocument>): String = builder(query, documents)

    companion object {
        /** Provides numbered context passages followed by the question. */
        val default = PromptTemplate { query, documents ->
            buildString {
                append("Use the following context to answer the question.\n\n")
                append("Context:\n")
                documents.forEachIndexed { index, doc ->
                    append("[${index + 1}] ${doc.text}\n\n")
                }
                append("Question: $query\n")
                append("Answer based on the context above:")
            }
        }

        /** Minimal prompt for short, direct answers. */
        val concise = PromptTemplate { query, documents ->
            val context = documents.joinToString("\n") { it.text }
            "Context:\n$context\n\nQuestion: $query\nAnswer:"
        }

        /** Instructs the model to cite sources using [1], [2], etc. */
        val withCitations = PromptTemplate { query, documents ->
            buildString {
                append("Answer the question using the provided context. Cite sources using [1], [2], etc.\n\n")
                append("Context:\n")
                documents.forEachIndexed { index, doc ->
                    append("[${index + 1}] ${doc.text}\n\n")
                }
                append("Question: $query\n")
                append("Answer with citations:")
            }
        }
    }
}
