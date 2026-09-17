package ai.nobodywho.testapp

import ai.nobodywho.Chat
import ai.nobodywho.Message
import ai.nobodywho.Model
import ai.nobodywho.Tool
import ai.nobodywho.text
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Test
import org.junit.runner.RunWith

// Top-level function so Tool's KFunction reflection can bind to it. Tool only
// supports top-level / class / companion functions, not local ones.
fun ping(): String = "pong"

/**
 * On-device smoke test that runs on real hardware via Firebase Test Lab.
 *
 * Mirrors the chat / streaming / tool-calling assertions from the host-JVM
 * `IntegrationTest`, but exercises the arm64 Android `.so` on a physical phone.
 *
 * CI points `-e modelUrl` at an OBB it preloaded, so the device never downloads
 * the model. A local run takes the default and fetches it from Hugging Face.
 */
@RunWith(AndroidJUnit4::class)
class DeviceInferenceTest {

    // Overridable via `-e modelUrl <url-or-path>` so the workflow can point at a
    // preloaded model without recompiling; defaults to what the JVM tests use.
    private fun modelUrl(): String =
        InstrumentationRegistry.getArguments().getString("modelUrl")
            ?: "hf://NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf"

    @Test
    fun chatCompletesStreamsAndCallsTools() = runBlocking {
        // Ask for the GPU like a real app would. Android has no GPU backend
        // yet, so this falls back to CPU today and starts exercising the GPU
        // path automatically once one lands.
        val model: Model = Model.load(modelUrl(), useGpu = true)

        // Completion
        val chat = Chat(
            model = model,
            systemPrompt = "Reply with one word only.",
            templateVariables = mapOf("enable_thinking" to false),
        )
        val response: String = chat.ask("Say hello").completed()
        assertFalse("Completion should be non-empty", response.isEmpty())

        // Streaming
        chat.resetContext(systemPrompt = "Reply briefly.")
        val tokens: List<String> = chat.ask("Say hi").asFlow().toList()
        assertFalse("Streaming should yield at least one token", tokens.isEmpty())

        // Tool calling
        val pingTool: Tool = Tool(
            name = "ping",
            description = "Ping the server",
            function = ::ping,
        )
        chat.resetContext(
            systemPrompt = "Use the ping tool now.",
            tools = listOf(pingTool),
        )
        chat.ask("Ping the server").completed()
        val toolResponse: Message.Tool? =
            chat.getChatHistory().filterIsInstance<Message.Tool>().firstOrNull()
        assertNotNull("Expected a tool response in chat history", toolResponse)
        val toolText: String = toolResponse!!.content.text
        assertEquals("pong", toolText)
    }
}
