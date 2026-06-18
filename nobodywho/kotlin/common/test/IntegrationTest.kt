package ai.nobodywho

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.Assume
import org.junit.Assert.*
import org.junit.Test
import java.io.File

fun ping(): String = "pong"
suspend fun suspendPing(): String { delay(1); return "async pong" }
suspend fun slowSuspendPing(): String { delay(2000); return "slow async pong" }
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
    fun testSuspendToolConstruction() {
        val tool = Tool(
            name = "suspend_ping",
            description = "An async ping tool",
            function = ::suspendPing
        )
        assertNotNull(tool.getSchemaJson())
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

    @Test
    fun testTokenize() = runBlocking {
        val modelPath = requireEnv("TEST_MODEL")
        val model = Model.load(modelPath)
        val chat = Chat(model = model, templateVariables = mapOf("enable_thinking" to false))
        val tokens = chat.tokenize("Hey!")
        assertEquals(listOf<Int?>(18665, 0), tokens)
    }

    @Test
    fun testStats() = runBlocking {
        val modelPath = requireEnv("TEST_MODEL")
        val model = Model.load(modelPath)
        val chat = Chat(
            model = model,
            templateVariables = mapOf("enable_thinking" to false)
        )
        chat.ask("What is the capital of Denmark?").completed()
        val stats = chat.getStats()
        assertTrue("context_used should be > 0", stats.contextUsed > 0u)
        assertTrue("context_used should not exceed context_size", stats.contextUsed <= stats.contextSize)
    }

    @Test(timeout = 60_000)
    fun testSuspendToolDoesNotBlockCallerThread() = runBlocking {
        val modelPath = requireEnv("TEST_MODEL")
        val model = Model.load(modelPath)

        // Use a tool that takes 2 seconds — much longer than inference startup.
        // This guarantees the tool is still running when we check concurrency.
        val tool = Tool(
            name = "slow_suspend_ping",
            description = "Ping the server asynchronously (slow)",
            function = ::slowSuspendPing
        )
        val chat = Chat(
            model = model,
            systemPrompt = "Use the slow_suspend_ping tool now.",
            tools = listOf(tool),
            templateVariables = mapOf("enable_thinking" to false)
        )

        // runBlocking uses a single-threaded event loop. Coroutine B can only
        // run if coroutine A suspends (yields the thread). We check that B
        // completes while A is still in progress — proving .completed()
        // suspends rather than blocking the thread.
        val chatDone = CompletableDeferred<Unit>()
        val concurrentWorkDone = CompletableDeferred<Boolean>()

        launch {
            chat.ask("Ping the server").completed()
            chatDone.complete(Unit)
        }
        launch {
            delay(50)
            // If the thread were blocked by coroutine A, we'd never reach here.
            // Also verify A hasn't already finished (which would make the test meaningless).
            assertFalse("Chat should still be in progress when concurrent work runs", chatDone.isCompleted)
            concurrentWorkDone.complete(true)
        }

        assertTrue(
            "Concurrent coroutine must complete during inference — proves the caller thread is not blocked",
            concurrentWorkDone.await()
        )

        // Wait for chat to finish, then verify the suspend tool was actually invoked
        chatDone.await()
        val history = chat.getChatHistory()
        val toolResponse = history.firstOrNull { it is Message.Tool }
        assertNotNull("Suspend tool should have been called", toolResponse)
        assertEquals("slow async pong", (toolResponse as Message.Tool).content)
    }

    @Test
    fun testTranslator() = runBlocking {
        val modelPath = requireEnv("TEST_TRANSLATE_MODEL")
        val model = Model.load(modelPath)
        val translator = Translator(model = model, source = "en", target = "da")
        val result = translator.translate("Hello, how are you?").completed()
        assertTrue("Translation result should contain 'Hej'", result.contains("Hej"))
        translator.destroy()
    }
}
