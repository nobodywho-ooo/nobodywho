package ai.nobodywho.testapp

import ai.nobodywho.Chat
import ai.nobodywho.Message
import ai.nobodywho.Model
import ai.nobodywho.Tool
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
 * The model is fetched on-device by the binding itself: `Model.load(url)`
 * downloads the GGUF into the app's cache dir (app-writable, no permission,
 * no scoped-storage handling) and loads it. If the download or load fails the
 * test throws and the run goes red — the intended loud failure.
 */
@RunWith(AndroidJUnit4::class)
class DeviceInferenceTest {

    // Overridable via `-e modelUrl <url>` so the workflow can swap models
    // without recompiling; defaults to the same small model the JVM tests use.
    private fun modelUrl(): String =
        InstrumentationRegistry.getArguments().getString("modelUrl")
            ?: "hf://NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf"

    @Test
    fun chatCompletesStreamsAndCallsTools() = runBlocking {
        // useGpu = false: the Android .so has no GPU backend yet, so pin CPU for
        // a deterministic smoke test. Flip to true once a GPU backend lands.
        val model = Model.load(modelUrl(), useGpu = false)

        // Completion
        val chat = Chat(
            model = model,
            systemPrompt = "Reply with one word only.",
            templateVariables = mapOf("enable_thinking" to false),
        )
        val response = chat.ask("Say hello").completed()
        assertFalse("Completion should be non-empty", response.isEmpty())

        // Streaming
        chat.resetContext(systemPrompt = "Reply briefly.")
        val tokens = chat.ask("Say hi").asFlow().toList()
        assertFalse("Streaming should yield at least one token", tokens.isEmpty())

        // Tool calling
        val pingTool = Tool(
            name = "ping",
            description = "Ping the server",
            function = ::ping,
        )
        chat.resetContext(
            systemPrompt = "Use the ping tool now.",
            tools = listOf(pingTool),
        )
        chat.ask("Ping the server").completed()
        val toolResponse = chat.getChatHistory().firstOrNull { it is Message.Tool }
        assertNotNull("Expected a tool response in chat history", toolResponse)
        assertEquals("pong", (toolResponse as Message.Tool).content)
    }
}
