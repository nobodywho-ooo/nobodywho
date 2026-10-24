package ai.nobodywho

import java.io.Closeable
import org.json.JSONArray
import org.json.JSONObject
import uniffi.nobodywho.RustDecision as InternalDecision

/**
 * A question asked of a decision model. Matches the Laya question types: a
 * single choice, an ordered score, or a boolean (`noul`).
 */
sealed class DecisionQuestion {
    internal abstract fun toJson(): JSONObject

    /** Pick one option. [criteria] maps option ids to descriptions. */
    data class Choice(
        val instructions: Any,
        val criteria: Map<String, Any>,
    ) : DecisionQuestion() {
        override fun toJson(): JSONObject = JSONObject()
            .put("type", "choice")
            .put("instructions", decisionsJson(instructions))
            .put("criteria", JSONObject(criteria.mapValues { decisionsJson(it.value) }))
    }

    /** Score from lowest to highest. [criteria] holds 2 to 10 ordered descriptions. */
    data class Score(
        val instructions: Any,
        val criteria: List<Any>,
    ) : DecisionQuestion() {
        override fun toJson(): JSONObject = JSONObject()
            .put("type", "score")
            .put("instructions", decisionsJson(instructions))
            .put("criteria", JSONArray(criteria.map { decisionsJson(it) }))
    }

    /** A boolean question. [criteria] optionally describes the "false" and "true" answers. */
    data class Noul(
        val instructions: Any,
        val criteria: Map<String, Any>? = null,
    ) : DecisionQuestion() {
        override fun toJson(): JSONObject = JSONObject()
            .put("type", "noul")
            .put("instructions", decisionsJson(instructions))
            .apply { criteria?.let { put("criteria", JSONObject(it.mapValues { entry -> decisionsJson(entry.value) })) } }
    }
}

/** One answer to a [DecisionQuestion], with the probabilities behind it. */
sealed class DecisionAnswer {
    /** Picked option id, its probabilities, and the separation from a uniform distribution. */
    data class Choice(
        val choice: String,
        val probabilities: Map<String, Double>,
        val confidence: Double,
    ) : DecisionAnswer()

    /** Expected zero-based level index (can fall between levels). */
    data class Score(
        val score: Double,
        val legend: Map<String, Any?>,
        val probabilities: Map<String, Double>,
        val confidence: Double,
    ) : DecisionAnswer()

    /** Probability that the answer is "true". */
    data class Noul(val noul: Double) : DecisionAnswer()
}

/** Token usage for one [Decision.decide] call. */
data class DecisionUsage(val inputTokens: Int, val outputTokens: Int)

/** Answers keyed by question name, plus the token usage of the run. */
data class DecisionResponse(
    val answers: Map<String, DecisionAnswer>,
    val usage: DecisionUsage,
)

/**
 * Typed, non-generative decisions over text or JSON with a Laya GGUF model.
 * Every answer comes with probabilities instead of generated text.
 *
 * ```kotlin
 * val decision = Decision.fromPath("Laya-Q8_0.gguf")
 * val response = decision.decide(
 *     state = "I was charged twice. Please refund the duplicate payment.",
 *     questions = mapOf(
 *         "refund" to DecisionQuestion.Noul("Is a refund requested?"),
 *     ),
 * )
 * ```
 */
class Decision(
    model: Model,
    contextSize: UInt? = null
) : Closeable {
    private val inner: InternalDecision = InternalDecision(model.inner, contextSize)

    companion object {
        /** Create a decision handle directly from a model path. */
        suspend fun fromPath(
            modelPath: String,
            useGpu: Boolean = true,
            contextSize: UInt? = null
        ): Decision = Decision(Model.load(modelPath, useGpu), contextSize)
    }

    /** Run the [questions] over [state], which may be text, a map, or a list. */
    suspend fun decide(state: Any, questions: Map<String, DecisionQuestion>): DecisionResponse {
        val questionsJson = JSONObject(questions.mapValues { it.value.toJson() })
        val stateJson = if (state is String) JSONObject.quote(state) else decisionsJson(value = state).toString()
        val response = JSONObject(
            inner.decide(stateJson = stateJson, questionsJson = questionsJson.toString())
        )
        return decisionResponse(response)
    }

    /** Free the underlying Rust resources. */
    fun destroy() = inner.destroy()
    override fun close() { destroy() }
}

/** Normalize a Kotlin value into something JSON can hold. */
internal fun decisionsJson(value: Any?): Any = when (value) {
    null -> JSONObject.NULL
    is JSONObject, is JSONArray -> value
    is Map<*, *> -> JSONObject(value.mapKeys { it.key.toString() })
    is Iterable<*> -> JSONArray(value.map { decisionsJson(it) })
    else -> value
}

private fun jsonMap(json: JSONObject): Map<String, Any?> =
    json.keys().asSequence().associateWith { key ->
        json.opt(key).takeUnless { it == JSONObject.NULL }
    }

private fun jsonProbabilities(json: JSONObject): Map<String, Double> =
    json.keys().asSequence().associateWith { json.getDouble(it) }

private fun decisionResponse(response: JSONObject): DecisionResponse {
    val answers = response.getJSONObject("answers")
    val usage = response.getJSONObject("usage")
    return DecisionResponse(
        answers = answers.keys().asSequence().associateWith { name ->
            val answer = answers.getJSONObject(name)
            when (answer.getString("type")) {
                "choice" -> DecisionAnswer.Choice(
                    choice = answer.getString("choice"),
                    probabilities = jsonProbabilities(answer.getJSONObject("probabilities")),
                    confidence = answer.getDouble("confidence"),
                )
                "score" -> DecisionAnswer.Score(
                    score = answer.getDouble("score"),
                    legend = jsonMap(answer.getJSONObject("legend")),
                    probabilities = jsonProbabilities(answer.getJSONObject("probabilities")),
                    confidence = answer.getDouble("confidence"),
                )
                else -> DecisionAnswer.Noul(answer.getDouble("noul"))
            }
        },
        usage = DecisionUsage(
            inputTokens = usage.getInt("input_tokens"),
            outputTokens = usage.getInt("output_tokens"),
        ),
    )
}
