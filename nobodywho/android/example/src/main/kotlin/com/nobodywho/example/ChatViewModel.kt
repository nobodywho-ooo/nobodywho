package com.nobodywho.example

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.nobodywho.Chat
import com.nobodywho.ChatConfig
import com.nobodywho.NobodyWhoModel
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

data class Message(
    val text: String,
    val isUser: Boolean,
    val thinking: String? = null
)

class ChatViewModel(application: Application) : AndroidViewModel(application) {

    private val _messages = MutableStateFlow<List<Message>>(emptyList())
    val messages: StateFlow<List<Message>> = _messages.asStateFlow()

    private val _isReady = MutableStateFlow(false)
    val isReady: StateFlow<Boolean> = _isReady.asStateFlow()

    private val _isInitializing = MutableStateFlow(false)
    val isInitializing: StateFlow<Boolean> = _isInitializing.asStateFlow()

    private val _isGenerating = MutableStateFlow(false)
    val isGenerating: StateFlow<Boolean> = _isGenerating.asStateFlow()

    private val _statusMessage = MutableStateFlow("")
    val statusMessage: StateFlow<String> = _statusMessage.asStateFlow()

    private var chat: Chat? = null

    /** Initialize from a pre-loaded model. No-op if already initialized. */
    fun initialize(model: NobodyWhoModel, systemPrompt: String? = null) {
        if (_isReady.value || _isInitializing.value) return
        viewModelScope.launch {
            _isInitializing.value = true
            try {
                chat = withContext(Dispatchers.IO) {
                    Chat(model = model, config = ChatConfig(systemPrompt = systemPrompt))
                }
                _isReady.value = true
                _statusMessage.value = ""
            } catch (e: Exception) {
                _statusMessage.value = "Failed to initialize chat: ${e.message}"
            } finally {
                _isInitializing.value = false
            }
        }
    }

    fun sendMessage(input: String) {
        val chat = chat ?: return
        if (input.isBlank()) return

        _messages.value = _messages.value + Message(text = input, isUser = true)
        _isGenerating.value = true

        viewModelScope.launch {
            try {
                var accumulated = ""
                chat.askFlow(input).collect { token ->
                    accumulated += token
                    val (thinking, answer) = parseThinking(accumulated)
                    val current = _messages.value.toMutableList()
                    if (current.lastOrNull()?.isUser == false) {
                        current[current.lastIndex] = Message(
                            text = answer,
                            isUser = false,
                            thinking = thinking?.takeIf { it.isNotBlank() }
                        )
                    } else {
                        current.add(Message(text = answer, isUser = false, thinking = thinking))
                    }
                    _messages.value = current
                }
            } catch (e: Exception) {
                _messages.value = _messages.value + Message(text = "Error: ${e.message}", isUser = false)
            } finally {
                _isGenerating.value = false
            }
        }
    }

    private fun parseThinking(response: String): Pair<String?, String> {
        val open = response.indexOf("<think>")
        val close = response.indexOf("</think>")
        if (open == -1 || close == -1 || close < open) return null to response
        val thinking = response.substring(open + 7, close).trim()
        val answer = response.substring(close + 8).trim()
        return thinking to answer
    }
}
