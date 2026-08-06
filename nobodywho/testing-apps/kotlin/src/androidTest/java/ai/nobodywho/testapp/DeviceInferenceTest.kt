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
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

// Top-level function so Tool's KFunction reflection can bind to it. Tool only
// supports top-level / class / companion functions, not local ones.
fun ping(): String = "pong"

/**
 * On-device smoke test that runs on real hardware via Firebase Test Lab.
 *
 * This mirrors the chat / streaming / tool-calling assertions from the host-JVM
 * `IntegrationTest`, but exercises the arm64 Android `.so` on a physical phone.
 * Unlike the JVM test it does NOT skip when the model is absent — on a device
 * run a missing model means the harness is broken, so it must fail loudly.
 */
@RunWith(AndroidJUnit4::class)
class DeviceInferenceTest {

    /**
     * The `.gguf` is pushed onto the device by the CI workflow via
     * `gcloud ... --other-files`, and its device path is handed to us as an
     * instrumentation argument (`-e modelPath ...`). We fall back to the
     * app-specific external files dir, which is the only shared-storage
     * location an app can read without special permissions on API 30+.
     */
    private fun modelPath(): String {
        InstrumentationRegistry.getArguments().getString("modelPath")?.let { return it }
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        return File(ctx.getExternalFilesDir(null), "model.gguf").absolutePath
    }

    @Test
    fun chatCompletesStreamsAndCallsTools() = runBlocking {
        val path = modelPath()
        // Hard failure (not an assumption/skip): a missing model means the
        // device run itself is broken and we want a red result.
        assertTrue("Model not found on device at $path", File(path).exists())

        val model = Model.load(path)

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
