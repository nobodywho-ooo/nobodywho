package com.nobodywho.example

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.nobodywho.NobodyWhoModel
import com.nobodywho.loadNobodyWhoModel
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * Activity-scoped ViewModel that holds the loaded LLM weights.
 *
 * Both the Chat tab and the RAG tab call [loadIfNeeded]. The first call loads the
 * model from disk; all subsequent calls are no-ops, so the weights are only ever
 * in memory once regardless of how many times the user switches tabs.
 */
class SharedModelViewModel(application: Application) : AndroidViewModel(application) {

    private val _model = MutableStateFlow<NobodyWhoModel?>(null)
    val model: StateFlow<NobodyWhoModel?> = _model.asStateFlow()

    private val _isLoading = MutableStateFlow(false)
    val isLoading: StateFlow<Boolean> = _isLoading.asStateFlow()

    private val _loadError = MutableStateFlow<String?>(null)
    val loadError: StateFlow<String?> = _loadError.asStateFlow()

    fun loadIfNeeded(path: String, useGpu: Boolean = true) {
        if (_model.value != null || _isLoading.value) return
        viewModelScope.launch {
            _isLoading.value = true
            try {
                _model.value = withContext(Dispatchers.IO) {
                    loadNobodyWhoModel(path = path, useGpu = useGpu)
                }
            } catch (e: Exception) {
                _loadError.value = e.message
            } finally {
                _isLoading.value = false
            }
        }
    }
}
