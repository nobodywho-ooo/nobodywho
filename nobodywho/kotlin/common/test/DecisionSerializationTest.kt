package ai.nobodywho

import org.junit.Assert.assertEquals
import org.junit.Test

class DecisionSerializationTest {
    @Test
    fun keepsInputOrder() {
        assertEquals(
            "{\"z-message\":\"refund\",\"a-context\":{\"z\":1,\"a\":2}}",
            decisionsString(value = linkedMapOf("z-message" to "refund", "a-context" to linkedMapOf("z" to 1, "a" to 2))),
        )
        val questions = linkedMapOf(
            "z-route" to DecisionQuestion.Choice(
                instructions = linkedMapOf("z-task" to "route", "a-context" to "support"),
                criteria = linkedMapOf("technical" to "bugs", "billing" to "charges"),
            ),
            "a-refund" to DecisionQuestion.Noul(
                instructions = "Refund?",
                criteria = linkedMapOf("true" to "yes", "false" to "no"),
            ),
        )
        assertEquals(
            "{\"z-route\":{\"type\":\"choice\",\"instructions\":{\"z-task\":\"route\",\"a-context\":\"support\"},\"criteria\":{\"technical\":\"bugs\",\"billing\":\"charges\"}},\"a-refund\":{\"type\":\"noul\",\"instructions\":\"Refund?\",\"criteria\":{\"true\":\"yes\",\"false\":\"no\"}}}",
            decisionsQuestionsString(questions = questions),
        )
    }
}
