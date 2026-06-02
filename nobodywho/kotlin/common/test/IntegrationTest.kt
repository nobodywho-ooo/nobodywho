package ai.nobodywho

import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import org.junit.Assume
import org.junit.Assert.*
import org.junit.Test
import java.io.File

fun ping(): String = "pong"
fun badTool(url: java.net.URL): String = url.toString()

class IntegrationTest {

    private fun requireEnv(name: String): String {
        val value = System.getenv(name)
        Assume.assumeNotNull(value)
        return value!!
    }

    @Test(expected = IllegalArgumentException::class)
    fun testUnsupportedToolParameterType() {
        Tool(
            name = "bad",
            description = "This should fail",
            function = ::badTool
        )
    }

    @Test
    fun testDownloadModel() = runBlocking {
        val modelUrl = "hf://NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf"

        // First download to discover the cache path, then delete to force a fresh download
        val cachedPath = Model.download(modelUrl)
        File(cachedPath).delete()

        var progressCalled = false
        val localPath = Model.download(modelUrl) { downloaded, total ->
            progressCalled = true
            assertTrue("downloaded should be <= total", downloaded <= total)
        }
        assertTrue("Downloaded file should exist", File(localPath).exists())
        assertTrue("Downloaded file should be non-empty", File(localPath).length() > 0)
        assertTrue("Progress callback should have been called", progressCalled)
    }

    @Test
    fun testChat() = runBlocking {
        val modelPath = requireEnv("TEST_MODEL")
        val model = Model.load(modelPath)
        val chat = Chat(
            model = model,
            systemPrompt = "Reply with one word only.",
            templateVariables = mapOf("enable_thinking" to false)
        )

        // Completion
        val response = chat.ask("Say hello").completed()
        assertFalse(response.isEmpty())

        // Streaming
        chat.resetContext(systemPrompt = "Reply briefly.")
        val tokens = chat.ask("Say hi").asFlow().toList()
        assertFalse(tokens.isEmpty())

        // Tool calling
        val pingTool = Tool(
            name = "ping",
            description = "Ping the server",
            function = ::ping
        )
        chat.resetContext(
            systemPrompt = "Use the ping tool now.",
            tools = listOf(pingTool)
        )
        chat.ask("Ping the server").completed()
        val history = chat.getChatHistory()
        val toolResponse = history.firstOrNull { it is Message.Tool }
        assertNotNull("Expected a tool response in chat history", toolResponse)
        assertEquals("pong", (toolResponse as Message.Tool).content)
    }
}
