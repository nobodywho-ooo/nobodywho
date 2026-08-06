package ai.nobodywho.testapp

import android.Manifest
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Bundle
import android.os.Environment
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import androidx.core.content.FileProvider
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import ai.nobodywho.Chat
import ai.nobodywho.Model
import ai.nobodywho.Prompt
import ai.nobodywho.Tool
import kotlinx.coroutines.launch
import java.io.File
import java.io.FileOutputStream
import java.time.LocalDateTime
import java.time.ZoneId
import java.time.format.DateTimeFormatter

// Tool functions (must be top-level for Kotlin reflection)

fun getCurrentTime(timezone: String): String {
    return try {
        val zone = ZoneId.of(timezone)
        val now = LocalDateTime.now(zone)
        val formatter = DateTimeFormatter.ofPattern("yyyy-MM-dd HH:mm:ss")
        """{"time": "${now.format(formatter)}", "timezone": "$timezone"}"""
    } catch (e: Exception) {
        """{"error": "Unknown timezone: $timezone"}"""
    }
}

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // Disable the Mali coopmat matmul path (its prompt-processing cliff makes
        // long-prompt prefill ~4x slower). Must be set before the Vulkan backend
        // initializes on first model load; native ggml reads it via getenv().
        try {
            android.system.Os.setenv("GGML_VK_DISABLE_COOPMAT", "1", true)
        } catch (e: Exception) {
        }
        setContent { MaterialTheme { ChatApp() } }
    }
}

data class ChatMessage(val role: String, val content: String)

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ChatApp() {
    val scope = rememberCoroutineScope()
    val context = LocalContext.current

    // State
    var modelPath by remember { mutableStateOf("/sdcard/models/model.gguf") }
    var mmprojPath by remember { mutableStateOf("") }
    var chat by remember { mutableStateOf<Chat?>(null) }
    var loading by remember { mutableStateOf(false) }
    var statusText by remember { mutableStateOf("Enter a model path to get started.") }
    var input by remember { mutableStateOf("") }
    var currentResponse by remember { mutableStateOf("") }
    var generating by remember { mutableStateOf(false) }
    var useGpu by remember { mutableStateOf(true) }
    var lastPerf by remember { mutableStateOf("") }
    var threadsText by remember { mutableStateOf("") }
    var activeThreads by remember { mutableStateOf("") }
    val messages = remember { mutableStateListOf<ChatMessage>() }
    val listState = rememberLazyListState()

    // Camera state
    var photoUri by remember { mutableStateOf<Uri?>(null) }
    var photoPath by remember { mutableStateOf<String?>(null) }

    val cameraLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.TakePicture()
    ) { success ->
        if (!success) {
            photoUri = null
            photoPath = null
        } else if (photoPath != null) {
            // Downscale to 512x512 to speed up vision processing
            val original = BitmapFactory.decodeFile(photoPath)
            if (original != null) {
                val scaled = Bitmap.createScaledBitmap(original, 512, 512, true)
                FileOutputStream(photoPath!!).use { out ->
                    scaled.compress(Bitmap.CompressFormat.JPEG, 85, out)
                }
                original.recycle()
                scaled.recycle()
            }
        }
    }

    val permissionLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestPermission()
    ) { granted ->
        if (granted) {
            val file = File(context.cacheDir, "images").also { it.mkdirs() }
                .let { File(it, "photo_${System.currentTimeMillis()}.jpg") }
            val uri = FileProvider.getUriForFile(context, "${context.packageName}.fileprovider", file)
            photoUri = uri
            photoPath = file.absolutePath
            cameraLauncher.launch(uri)
        }
    }

    fun takePhoto() {
        if (ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA)
            == PackageManager.PERMISSION_GRANTED
        ) {
            val file = File(context.cacheDir, "images").also { it.mkdirs() }
                .let { File(it, "photo_${System.currentTimeMillis()}.jpg") }
            val uri = FileProvider.getUriForFile(context, "${context.packageName}.fileprovider", file)
            photoUri = uri
            photoPath = file.absolutePath
            cameraLauncher.launch(uri)
        } else {
            permissionLauncher.launch(Manifest.permission.CAMERA)
        }
    }

    fun sendMessage() {
        val text = input.trim()
        if (text.isEmpty() || chat == null || generating) return

        val attachedPhoto = photoPath
        photoPath = null
        photoUri = null

        val displayText = if (attachedPhoto != null) "[photo] $text" else text
        messages.add(ChatMessage("user", displayText))
        input = ""
        generating = true
        currentResponse = ""

        scope.launch {
            try {
                val stream = if (attachedPhoto != null) {
                    chat!!.ask(Prompt(Prompt.Image(attachedPhoto), Prompt.Text(text)))
                } else {
                    chat!!.ask(text)
                }
                var tokenCount = 0
                var firstTokenAt = 0L
                val askStart = System.currentTimeMillis()
                stream.asFlow().collect { token ->
                    if (firstTokenAt == 0L) firstTokenAt = System.currentTimeMillis()
                    tokenCount++
                    currentResponse += token
                }
                val ttftSec = if (firstTokenAt > 0L) (firstTokenAt - askStart) / 1000.0 else 0.0
                val genMs = System.currentTimeMillis() - firstTokenAt
                val tps = if (genMs > 0 && tokenCount > 1) (tokenCount - 1) * 1000.0 / genMs else 0.0
                lastPerf = "ttft %.2fs · %d tokens · %.1f tok/s · threads=%s · %s".format(
                    ttftSec, tokenCount, tps, activeThreads, if (useGpu) "GPU" else "CPU"
                )
                messages.add(ChatMessage("assistant", currentResponse))
                currentResponse = ""
            } catch (e: Exception) {
                messages.add(ChatMessage("error", e.message ?: "Unknown error"))
            }
            generating = false
            listState.animateScrollToItem(messages.size - 1)
        }
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("NobodyWho Test") },
                actions = {
                    if (chat != null) {
                        TextButton(onClick = {
                            chat = null
                            messages.clear()
                            currentResponse = ""
                            generating = false
                        }) {
                            Text("Reset")
                        }
                    }
                }
            )
        }
    ) { padding ->
        Column(
            modifier = Modifier
                .padding(padding)
                .fillMaxSize()
                .padding(16.dp)
        ) {
            // Model loading section
            if (chat == null) {
                OutlinedTextField(
                    value = modelPath,
                    onValueChange = { modelPath = it },
                    label = { Text("Model path (.gguf)") },
                    modifier = Modifier.fillMaxWidth(),
                    singleLine = true
                )
                Spacer(modifier = Modifier.height(8.dp))
                OutlinedTextField(
                    value = mmprojPath,
                    onValueChange = { mmprojPath = it },
                    label = { Text("Vision projector path (optional)") },
                    modifier = Modifier.fillMaxWidth(),
                    singleLine = true
                )
                Spacer(modifier = Modifier.height(8.dp))
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Text("Use GPU (Vulkan)", modifier = Modifier.weight(1f))
                    Switch(checked = useGpu, onCheckedChange = { useGpu = it })
                }
                Spacer(modifier = Modifier.height(8.dp))
                OutlinedTextField(
                    value = threadsText,
                    onValueChange = { threadsText = it },
                    label = { Text("Threads (blank = auto / physical cores)") },
                    modifier = Modifier.fillMaxWidth(),
                    singleLine = true
                )
                Spacer(modifier = Modifier.height(8.dp))
                Button(
                    onClick = {
                        loading = true
                        statusText = "Loading model (${if (useGpu) "GPU" else "CPU"})..."
                        scope.launch {
                            try {
                                val proj = mmprojPath.trim().ifEmpty { null }
                                val timeTool = Tool(
                                    name = "get_current_time",
                                    description = "Get the current date and time for a given timezone (e.g. 'UTC', 'Europe/London', 'America/New_York')",
                                    function = ::getCurrentTime
                                )
                                val threadCount = threadsText.trim().toUIntOrNull()
                                activeThreads = threadCount?.toString() ?: "auto"
                                val model = Model.load(modelPath.trim(), useGpu = useGpu, projectionModelPath = proj)
                                chat = Chat(
                                    model = model,
                                    tools = listOf(timeTool),
                                    threadCount = threadCount
                                )
                                statusText = "Model loaded (${if (useGpu) "GPU" else "CPU"}, threads=$activeThreads)."
                            } catch (e: Exception) {
                                statusText = "Error: ${e.message}"
                            }
                            loading = false
                        }
                    },
                    enabled = modelPath.isNotBlank() && !loading,
                    modifier = Modifier.fillMaxWidth()
                ) {
                    if (loading) {
                        CircularProgressIndicator(modifier = Modifier.size(20.dp), strokeWidth = 2.dp)
                        Spacer(modifier = Modifier.width(8.dp))
                    }
                    Text("Load Model")
                }
                Spacer(modifier = Modifier.height(8.dp))
                Text(statusText, style = MaterialTheme.typography.bodySmall)
            }

            // Chat section
            if (chat != null) {
                LazyColumn(
                    state = listState,
                    modifier = Modifier.weight(1f).fillMaxWidth(),
                    verticalArrangement = Arrangement.spacedBy(8.dp)
                ) {
                    items(messages) { msg ->
                        val color = when (msg.role) {
                            "user" -> MaterialTheme.colorScheme.primaryContainer
                            "assistant" -> MaterialTheme.colorScheme.secondaryContainer
                            else -> MaterialTheme.colorScheme.errorContainer
                        }
                        Surface(
                            color = color,
                            shape = MaterialTheme.shapes.medium,
                            modifier = Modifier.fillMaxWidth()
                        ) {
                            Text(
                                text = msg.content,
                                modifier = Modifier.padding(12.dp),
                                style = MaterialTheme.typography.bodyMedium
                            )
                        }
                    }
                    // Show streaming response
                    if (currentResponse.isNotEmpty()) {
                        item {
                            Surface(
                                color = MaterialTheme.colorScheme.secondaryContainer,
                                shape = MaterialTheme.shapes.medium,
                                modifier = Modifier.fillMaxWidth()
                            ) {
                                Text(
                                    text = currentResponse,
                                    modifier = Modifier.padding(12.dp),
                                    style = MaterialTheme.typography.bodyMedium
                                )
                            }
                        }
                    }
                }

                Spacer(modifier = Modifier.height(8.dp))

                // Generation speed of the last message
                if (lastPerf.isNotEmpty()) {
                    Text(lastPerf, style = MaterialTheme.typography.bodySmall)
                    Spacer(modifier = Modifier.height(4.dp))
                }

                // Photo attachment indicator
                if (photoPath != null) {
                    Surface(
                        color = MaterialTheme.colorScheme.tertiaryContainer,
                        shape = MaterialTheme.shapes.small
                    ) {
                        Text(
                            text = "Photo attached",
                            modifier = Modifier.padding(8.dp),
                            style = MaterialTheme.typography.bodySmall
                        )
                    }
                    Spacer(modifier = Modifier.height(4.dp))
                }

                // Input row
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    // Camera button (only if vision model loaded)
                    if (mmprojPath.isNotBlank()) {
                        IconButton(onClick = { takePhoto() }) {
                            Text("📷")
                        }
                    }
                    OutlinedTextField(
                        value = input,
                        onValueChange = { input = it },
                        modifier = Modifier.weight(1f),
                        placeholder = { Text("Message...") },
                        singleLine = true,
                        enabled = !generating
                    )
                    Spacer(modifier = Modifier.width(8.dp))
                    Button(
                        onClick = { sendMessage() },
                        enabled = input.isNotBlank() && !generating
                    ) {
                        Text("Send")
                    }
                }
            }
        }
    }
}
