package com.nobodywho.example

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.viewModels
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Chat
import androidx.compose.material.icons.filled.Description
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.lifecycle.compose.collectAsStateWithLifecycle

class MainActivity : ComponentActivity() {

    // SharedModelViewModel is Activity-scoped: one instance, survives tab switches.
    private val sharedModelViewModel: SharedModelViewModel by viewModels()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        // Start loading the model as early as possible.
        sharedModelViewModel.loadIfNeeded(
            path = "/data/local/tmp/model.gguf",
            useGpu = true
        )

        setContent {
            NobodyWhoTheme {
                Surface(
                    modifier = Modifier.fillMaxSize(),
                    color = MaterialTheme.colorScheme.background
                ) {
                    MainScreen(sharedModelViewModel = sharedModelViewModel)
                }
            }
        }
    }
}

private data class Tab(val label: String, val icon: ImageVector)

@Composable
private fun MainScreen(sharedModelViewModel: SharedModelViewModel) {
    val sharedModel by sharedModelViewModel.model.collectAsStateWithLifecycle()

    val tabs = listOf(
        Tab("Chat", Icons.Default.Chat),
        Tab("RAG", Icons.Default.Description)
    )
    var selectedTab by remember { mutableIntStateOf(0) }

    Scaffold(
        bottomBar = {
            NavigationBar {
                tabs.forEachIndexed { index, tab ->
                    NavigationBarItem(
                        selected = selectedTab == index,
                        onClick = { selectedTab = index },
                        icon = { Icon(tab.icon, contentDescription = tab.label) },
                        label = { Text(tab.label) }
                    )
                }
            }
        }
    ) { padding ->
        // sharedModel is the single NobodyWhoModel instance. Both screens receive it
        // and will not initialize until it is non-null, preventing duplicate loads.
        when (selectedTab) {
            0 -> ChatScreen(paddingValues = padding, sharedModel = sharedModel)
            1 -> RAGScreen(paddingValues = padding, sharedModel = sharedModel)
        }
    }
}

@Composable
private fun NobodyWhoTheme(content: @Composable () -> Unit) {
    MaterialTheme(content = content)
}
