# NobodyWho Unity

A Unity interface for running local AI chat models using the NobodyWho library.

## What is NobodyWho?

NobodyWho is a Unity package that allows you to run large language models (LLMs) locally in your Unity projects. It provides:
- **Local AI chat** - No internet connection required
- **GPU acceleration** - Uses your graphics card for faster inference
- **Real-time streaming** - See AI responses as they're generated
- **Customizable prompts** - Control the AI's behavior and personality
- **Customizable Structured Output** - Enforce specific outputs, like json or your own custom format
- **Embeddings and semantic comparisons** - Compare how close a string(ie. a user input) is to a specic sentence or string, useful for triggering events based on what the user says


## Quick Start Guide

### 1. Scene setup for chat node

The chat interface requires two main components:

**Model Component** - Loads and manages the AI model
**Chat Component** - Handles conversations and connects to the UI

### 2. Configure the NobodyWhoModel Component

1. **Add Model to Scene**: Under the Object Menu find Nobodywho and select the `Model` component
2. **Set Model Path**: Point to your `.gguf` model file, this must in the StreamingAssets folder (e.g., `root:/Assets/StreamingAssets/model.gguf`)
3. **Enable GPU**: Check `Use GPU If Available` for faster performance


### 3. Setup the NobodyWhoChat

1. **Add Chat to Scene**: Under the Object Menu find Nobodywho and select the `Chat` component
2. **Select the Model**: Click the Object selector in the Inspector and selec the `Model` object you just added
3. **Set the Prompt**: set the prompt to whatever you like, this will guide the LLM in what it should say and how it should say it.

### 4. send a message ###

1. **Add a Chat Controller**: Attach a script to the `Chat`.
2. **Get the Chat**: 


```csharp
using UnityEngine;
using NobodyWho;

public class SimpleChatController : MonoBehaviour
{
    public Chat chat;

    public void Start()
    {
        chat = GetComponent<NobodyWho.Chat>
        chat.StartWorker();
        chat.responseUpdated.AddListener(OnResponseUpdated);
        chat.responseFinished.AddListener(OnResponseFinished);

        chat.say("hello world")
    }

    private void OnResponseUpdated(token)
    {
        // prints the token as they are generated
        Debug.log(token)
    }

    private void OnResponseFinished(response)
    {
        // prints the full generated response
        Debug.log(reponse)
    }
    
}

```