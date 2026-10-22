package ai.nobodywho

import java.io.Closeable
import org.json.JSONObject
import org.json.JSONStringer
import uniffi.nobodywho.RustDecision as InternalDecision

/**
 * A question asked of a decision model. Matches the Laya question types: a
 * single choice, an ordered score, or a boolean (`noul`).
 */
sealed class DecisionQuestion {
    internal abstract fun writeJson(json: JSONStringer)

    /** Pick one option. [criteria] maps option ids to descriptions. */
    data class Choice(
        val instructions: Any,
        val criteria: Map<String, Any>,
    ) : DecisionQuestion() {
        override fun writeJson(json: JSONStringer) {
            json.`object`().key("type").value("choice")
            json.key("instructions")
            writeDecisionsJson(json = json, value = instructions)
            json.key("criteria")
            writeDecisionsJson(json = json, value = criteria)
            json.endObject()
        }
    }

    /** Score from lowest to highest. [criteria] holds 2 to 10 ordered descriptions. */
    data class Score(
        val instructions: Any,
        val criteria: List<Any>,
    ) : DecisionQuestion() {
        override fun writeJson(json: JSONStringer) {
            json.`object`().key("type").value("score")
            json.key("instructions")
            writeDecisionsJson(json = json, value = instructions)
            json.key("criteria")
            writeDecisionsJson(json = json, value = criteria)
            json.endObject()
        }
    }

    /** A boolean question. [criteria] optionally describes the "false" and "true" answers. */
    data class Noul(
        val instructions: Any,
        val criteria: Map<String, Any>? = null,
    ) : DecisionQuestion() {
        override fun writeJson(json: JSONStringer) {
            json.`object`().key("type").value("noul")
            json.key("instructions")
            writeDecisionsJson(json = json, value = instructions)
            criteria?.let {
                json.key("criteria")
                writeDecisionsJson(json = json, value = it)
            }
            json.endObject()
        }
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
        val response = JSONObject(
            inner.decide(
                stateJson = decisionsString(value = state),
                questionsJson = decisionsQuestionsString(questions = questions),
            )
        )
        return decisionResponse(response)
    }

    /** Free the underlying Rust resources. */
    fun destroy() = inner.destroy()
    override fun close() { destroy() }
}

/** Serialize Kotlin values without discarding map iteration order. */
internal fun decisionsString(value: Any?): String {
    val json = JSONStringer()
    json.array()
    writeDecisionsJson(json = json, value = value)
    json.endArray()
    val wrapped = json.toString()
    return wrapped.substring(startIndex = 1, endIndex = wrapped.length - 1)
}

internal fun decisionsQuestionsString(questions: Map<String, DecisionQuestion>): String {
    val json = JSONStringer()
    json.`object`()
    questions.forEach { (name, question) ->
        json.key(name)
        question.writeJson(json = json)
    }
    json.endObject()
    return json.toString()
}

internal fun writeDecisionsJson(json: JSONStringer, value: Any?) {
    when (value) {
        null -> json.value(JSONObject.NULL)
        is Map<*, *> -> {
            json.`object`()
            value.forEach { (key, nested) ->
                json.key(key.toString())
                writeDecisionsJson(json = json, value = nested)
            }
            json.endObject()
        }
        is Iterable<*> -> {
            json.array()
            value.forEach { writeDecisionsJson(json = json, value = it) }
            json.endArray()
        }
        is Array<*> -> writeDecisionsJson(json = json, value = value.asList())
        else -> json.value(value)
    }
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
