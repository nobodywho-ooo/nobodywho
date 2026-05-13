package com.nobodywho

import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import org.junit.Assume
import org.junit.Assert.*
import org.junit.Test

fun ping(): String = "pong"

class IntegrationTest {

    private fun requireEnv(name: String): String {
        val value = System.getenv(name)
        Assume.assumeNotNull(value)
        return value!!
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
