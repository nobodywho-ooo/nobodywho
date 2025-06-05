# NobodyWho Unity

A Unity interface for running local AI chat models using the NobodyWho library.

## What is NobodyWho?

NobodyWho is a Unity package that allows you to run large language models (LLMs) locally in your Unity projects. It provides:
- **Local AI chat** - No internet connection required
- **GPU acceleration** - Uses your graphics card for faster inference
- **Real-time streaming** - See AI responses as they're generated
- **Customizable prompts** - Control the AI's behavior and personality
- **Customizable Structured Output** - Enforce specific outputs, like json or your own custom format
- **Embeddings and semantic comparisons** - Compare how close a string(ie. a user input) is to a specific sentence or string, useful for triggering events based on what the user says

## Sample Scenes

This package includes two demo scenes:

### 1. Chat Interface (`Assets/Samples/Chat/`)
A complete chat interface showing real-time AI conversation.

### 2. Embeddings Demo (`Assets/Samples/Embeddings/`)
A semantic similarity analyzer that shows how close your input text is to predefined phrases.

## Quick Start Guide

### Chat Setup

The chat interface requires two main components:

**Model Component** - Loads and manages the AI model
**Chat Component** - Handles conversations and connects to the UI

#### Configure the NobodyWhoModel Component

1. **Add Model to Scene**: Under the Object Menu find Nobodywho and select the `Model` component
2. **Set Model Path**: Point to your `.gguf` model file, this must be in the StreamingAssets folder (e.g., `root:/Assets/StreamingAssets/model.gguf`)
3. **Enable GPU**: Check `Use GPU If Available` for faster performance

#### Setup the NobodyWhoChat

1. **Add Chat to Scene**: Under the Object Menu find Nobodywho and select the `Chat` component
2. **Select the Model**: Click the Object selector in the Inspector and select the `Model` object you just added
3. **Set the Prompt**: Set the prompt to whatever you like, this will guide the LLM in what it should say and how it should say it.

#### Send a Message

```csharp
using UnityEngine;
using NobodyWho;

public class SimpleChatController : MonoBehaviour
{
    public Chat chat;

    public void Start()
    {
        chat = GetComponent<NobodyWho.Chat>();
        chat.StartWorker();
        chat.responseUpdated.AddListener(OnResponseUpdated);
        chat.responseFinished.AddListener(OnResponseFinished);

        chat.Say("hello world");
    }

    private void OnResponseUpdated(string token)
    {
        // prints the token as they are generated
        Debug.Log(token);
    }

    private void OnResponseFinished(string response)
    {
        // prints the full generated response
        Debug.Log(response);
    }
}
```

## Embeddings

Embeddings allow you to measure semantic similarity between pieces of text. This is useful for:
- **Intent detection** - Understanding what a user wants to do
- **Content matching** - Finding similar content
- **Trigger systems** - Activating events based on user input meaning

### Compare sentence similarity

```csharp
using UnityEngine;
using NobodyWho;

public class SimpleEmbeddingsController : MonoBehaviour
{
    public Embedding embedding;
    private float[] storedEmbeddings;

    public void Start()
    {
        embedding = GetComponent<NobodyWho.Embedding>();
        embedding.StartWorker();
        embedding.onEmbeddingComplete.AddListener(OnEmbeddingComplete);

        // Embed some text
        embedding.Embed("I love playing games");
    }

    private void OnEmbeddingComplete(float[] embeddingResult)
    {
        storedEmbeddings. # TODO: use arrays here.
        Debug.Log($"Generated embedding with {embeddingResult.Length} dimensions");
    }

    public void CompareTexts(float[] embedding1, float[] embedding2)
    {
        float similarity = embedding.CosineSimilarity(embedding1, embedding2);
        Debug.Log($"Similarity: {similarity} (Range: -1 to 1, higher = more similar)");
    }

    public void CompareVsLastEmbedding
}
```

**Note: you can either use the the unity event or get embeddings blocking. However we do not reccomend using the blocking approach as it might freeze across several frames**

### Embeddings Demo Scene

The embeddings demo shows real-time semantic similarity comparison:

1. **Enter text** in the left input field
2. **Click "Analyze Similarity"** to generate embeddings
3. **View results** as colored bars showing similarity to predefined phrases
4. **Green bars** indicate high similarity, **red bars** indicate low similarity

**Example**: Try entering "I enjoy gaming" and see how it scores high similarity with "I love playing video games" but low similarity with "The weather is beautiful today".
