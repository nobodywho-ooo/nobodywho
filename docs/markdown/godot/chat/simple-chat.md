# Simple Chat

_A comprehensive guide to configuring, streaming, and controlling LLM responses through the Chat component._


---

Great! You've completed the ["Getting Started"](../getting-started.md) guide and got your first chat working as well as a basic understanding of the vocabulary.   
Now let's dive deeper into the Chat component and show you all the settings and techniques you'll actually use when working with LLMs.
 
The Chat component isn't just for conversations - it's your main interface for any kind of LLM processing, whether that's generating dialogue, analyzing text, creating content, or any other language task.

In this guide, you'll learn:

- The main settings that control LLM behavior
- How to handle LLM responses efficiently 
- Managing context and memory
- Controlling when and how the LLM stops generating


Before we get started, you'll hear these words being used:

| Term | Meaning |
| ---- | ------- |
| **Sampler** | The thing that controls how the LLM selects the next token during generation (temperature, top-p, etc.). |
| **Grammar or Structured Output** | A formal structure that constrains the LLM's output to a set `"vocabulary"`. |
| **GBNF** | GGML Backus-Naur Form - a way to define structured output formats. |

## Handling LLM Responses

### The System Prompt: Setting LLM Behavior 

You've used this already, but let's talk about making it really work for you. The system prompt defines how the LLM should behave:

```markdown

# Character-based behavior
system_prompt = """You are a sarcastic but brilliant wizard.
Your answers are always accurate, but delivered with a dry wit.
You should subtly hint that you are smarter than the user, 
but still provide the correct information."""

# Task-specific behavior
system_prompt = """You are a translation assistant.
You will be given text in any language. Your job is to translate 
it into formal, academic French.
Do not add any commentary or conversational text. 
Respond only with the translated text."""
```

**Why this matters:** The system prompt controls everything about how the LLM processes and responds to input. It's your primary tool for getting the behavior you want.


Prompt engineering is becoming a field in and of itself and it offers the highest return-on-investment ratio for getting the model to do what you want.


### GPU Usage: Speed Things Up

By default, NobodyWho tries to use your GPU if you have one. This makes everything much faster:

=== ":simple-godotengine: Godot"
    ```gdscript
    # This is already the default, but you can be explicit
    model.use_gpu_if_available = true
    ```

=== ":simple-unity: Unity"
    ```csharp
    // This is already the default, but you can be explicit
    model.useGpuIfAvailable = true;
    ```

**When to turn this off:** there are some scenarios where it might actually be better to use system ram: 

- If you don't need an immediate answer, and would prefer to use GPU resources for graphics.
- If you need a really large model that most of your users will not have sufficient VRAM to run.

### Context Length: How Much the LLM Remembers

The LLM maintains context (memory of the conversation/interaction), but only up to a point. The default is 4096 tokens (roughly 3000 words):

=== ":simple-godotengine: Godot"
    ```gdscript
    # Default is fine for most uses
    context_length = 4096
    
    # Increase for longer contexts
    context_length = 8192
    ```

=== ":simple-unity: Unity"
    ```csharp
    // Default is fine for most uses
    chat.contextLength = 4096;
    
    // Increase for longer contexts
    chat.contextLength = 8192;
    ```


**Trade-off:** Longer context = more memory usage. The general rule of thumb is to start with the default or less and only increase if you need the LLM to remember more.

**Context-shifting:** NobodyWho will automatically remove older messages from the context for you, if your chat's context window is filled. Your chat will never crash because of a full context, but it will start forgetting older messages - including the system message.

### Streaming Responses vs Waiting for Complete Output

You have two main approaches for handling LLM responses, and choosing the right one depends on your use case:

**Streaming** gives you each token as it's generated - good for user interfaces where you want immediate feedback.

**Waiting for complete responses** blocks until the full output is ready - good for when you need the entire response before doing something.

If you're implemeting an interactive chat, you likely want to do both:

- Show each token to the user as they arrive. This will make the chat feel a lot faster.
- Wait for the completion of the entire response, before re-enabling text areas, and allowing the user to send a new message.

=== ":simple-godotengine: Godot"
    ```gdscript
    var current_response = ""
    
    func _on_response_updated(token: String):
        current_response += token
        # Good for: UI updates, real-time feedback
        ui_label.text = current_response
    
    func _on_response_finished(response: String):
        # Good for: Final processing, logging, triggering next actions
        print(response)
        response = response.replace("<player>", player.name)
        trigger_next_game_event()
    ```

=== ":simple-unity: Unity"
    ```csharp
    private string currentResponse = "";
    
    void OnResponseUpdated(string token)
    {
        currentResponse += token;
        // Good for: UI updates, real-time feedback
        uiText.text = currentResponse;
    }
    
    void OnResponseFinished(string response)
    {
        // Good for: Final processing, logging, triggering next actions
        response = response.Replace("<player>", Player.name);
        Debug.log(response);
        TriggerNextGameEvent();
    }
    ```

**When to use streaming:**
- Interactive dialogue where users expect immediate feedback
- Long responses where you want to show progress

**When to wait for complete responses:**
- When you need to make decisions based on the full LLM output
- Content generation where partial results are useless (like json or structured output answers).

you most likely end up using both; having the response_updated to stream to your UI and then triggering the next step in your program when you get the full response.

## Managing Context and Memory

Sometimes you need to reset the LLM's memory or manage what it remembers.

### Starting Fresh

=== ":simple-godotengine: Godot"
    ```gdscript
    # Clear all context, it will still have all the settings that you 
    # have set up before (including the system prompt)
    reset_context()
    ```

=== ":simple-unity: Unity"
    ```csharp
    // Clear all context, it will still have all the settings that you  
    // have set up before (including the system prompt)
    chat.ResetContext();
    ```

This is useful when:
- Starting a new task that's unrelated to previous ones, where the previous history is irrelevant
- The LLM gets confused as it has context shifted too much

### Advanced Context Management

If you need more control over what the LLM remembers:


=== ":simple-godotengine: Godot"

    ```gdscript
    # See what's in the context
    var messages = await get_chat_history()
    for message in messages:
        print(message.role, ": ", message.content)
    
    # Set a custom context (useful for templates or saved states)
    var task_context = [
        {"role": "user", "content": "Analyze the following data:"},
        {"role": "assistant", "content": "I'm ready to analyze data. Please provide it."},
        {"role": "user", "content": "Here's the data: " + data_to_analyze}
    ]
    set_chat_history(task_context)
    ```

=== ":simple-unity: Unity"

    ```csharp
    var history = new Chat.History(
        new List<Chat.Message>
        {
            new NobodyWho.Chat.Message("system", "You need to always remember the word: 'Cucumber'"),
            new NobodyWho.Chat.Message("user", "what is the word?"),
            new NobodyWho.Chat.Message("assistant", "Cucumber"),
        }
    );
    chat.SetHistory(history);
    ```
    or 

    ```csharp
    var history = await chat.GetHistory();
    history.messages[2].content = "I am the captain now"
    chat.SetHistory(history);
    ```



### Stop Words: Controlling LLM Output

Some LLMs can be very verbose and you might want the LLM to limit its verbosity and stop generating at specific words or phrases:

=== ":simple-godotengine: Godot"
    ```gdscript
    chat.system_prompt = "Always answer with a question, no matter what."
    chat.stop_words = PackedStringArray(["?"])
    chat.say("I think we should plan something special for Sarah's birthday. any ideas?")
    
    # The LLM will generate something like:
    # llm: "What do you think about a surprise party?" (stops here)
    ```

=== ":simple-unity: Unity"
    ```csharp
    chat.systemPrompt = "Always answer with a question, no matter what.";
    chat.stopWords = "?";
    chat.Say("I think we should plan something special for Sarah's birthday. any ideas?");
    
    // The LLM will generate something like:
    // llm: "What do you think about a surprise party?" (stops here)
    ```

This is useful when you want to prevent the LLM from running on. You can end generation prematurely, conditioned on specific words.

### Enforce Structured Output (JSON)

For reliable data extraction, you can force the LLM to output a response that strictly follows a basic JSON structure. This is incredibly useful for parsing LLM output into usable data without complex string matching.

When you enable grammar without providing a custom grammar string, the system defaults to a built-in JSON grammar that ensures valid JSON output.

=== ":simple-godotengine: Godot"
    
    ```gdscript
    # configure the sampler to use the json preset
    chat.sampler.set_preset_json()

    # Tell the LLM to provide structured data
    chat.system_prompt = """You are a character creator. 
    Generate a character with name, weapon, and armor properties."""
    chat.say("Create a fantasy character")

    # Expected output will be valid JSON, like:
    # {"name": "Eldara", "weapon": "enchanted bow", "armor": "leather vest"}
    ```

=== ":simple-unity: Unity"
    
    ```csharp
    // Configure the chat for JSON output
    chat.systemPrompt = @"You are a character creator. 
    Generate a character with name, weapon, and armor properties.";
    chat.useGrammar = true;
    // The grammar property can be left empty to use the default JSON grammar

    chat.Say("Create a fantasy character");

    // Expected output will be valid JSON, like:
    // {"name": "Eldara", "weapon": "enchanted bow", "armor": "leather vest"}
    ```

**Note:** For advanced use cases where you need a very specific JSON structure or structured output that is not JSON, you can provide your own custom GBNF grammar by setting the `gbnf_grammar` property (Godot) or `grammar` field (Unity). This is covered in the [Structured Output](structured-output.md) guide.

## Performance and Memory Tips

### Start the Worker Early

In a real-time application, you don't want the user's first interaction to trigger a long loading time. Starting the worker early, like during a splash screen or initial setup, pre-loads the model into memory so the first response is fast.

=== ":simple-godotengine: Godot"
    ```gdscript
    # In your _ready() function, set up everything before the app starts.
    func _ready():
        # 1. Configure the chat behavior
        self.system_prompt = "You are a helpful assistant."
        self.model_node = get_node("../SharedModel")

        # 2. Start the worker *before* the user can interact.
        # This pre-loads the model so the first interaction isn't slow.
        start_worker()

        # 3. Now other setup can happen
        print("Assistant chat is ready.")
    ```

=== ":simple-unity: Unity"
    ```csharp
    // In your Start() method, set up everything before the app starts.
    void Start()
    {
        // 1. Configure the chat behavior 
        chat.systemPrompt = "You are a helpful assistant.";
        // The model would also be assigned in the editor.

        // 2. Start the worker *before* the user can interact.
        // This pre-loads the model so the first interaction isn't slow.
        chat.StartWorker();

        // 3. Now other setup can happen
        Debug.Log("Assistant chat is ready.");
    }
    ```

**Why:** Starting the worker loads the model into memory. It's slow the first time, but then all LLM operations are much faster. 
You should definitely think about when to do this to not ruin the UX too much.

### Share Models Between Components

An application might need to use an LLM for several different tasks. Instead of loading the same heavy model multiple times, you can have multiple `Chat` components that all share a single `Model` component. Each `Chat` can have its own system prompt and configuration, directing it to perform a different task.

=== ":simple-godotengine: Godot"
    ```gdscript
    # An application with multiple LLM-powered behaviors, all sharing one model.

    func _ready():
        # 1. Get the single, shared model
        var shared_model = get_node("../SharedModel")

        # 2. Configure a chat component for general conversation
        var casual_chat = get_node("CasualChat")
        casual_chat.model_node = shared_model
        casual_chat.system_prompt = "You are a friendly and helpful assistant. Keep your answers concise."
        casual_chat.start_worker()

        # 3. Configure another chat component for structured data extraction
        var extractor_chat = get_node("ExtractorChat")
        extractor_chat.model_node = shared_model
        extractor_chat.system_prompt = "Extract the key information from the user's text and provide it in JSON format."
        # This one would likely use a grammar to enforce JSON output.
        extractor_chat.start_worker()

        # Now you can use both for different tasks without loading two models!
        casual_chat.say("Can you tell me about your capabilities?")
        extractor_chat.say("My name is Jane Doe and my email is jane@example.com.")
    ```

=== ":simple-unity: Unity"
    ```csharp
    // An application with multiple LLM-powered behaviors, all sharing one model.
    public Model sharedModel;
    public Chat casualChat;
    public Chat extractorChat;

    void Start()
    {
        // 1. Configure a chat component for general conversation
        casualChat.model = sharedModel;
        casualChat.systemPrompt = "You are a friendly and helpful assistant. Keep your answers concise.";
        casualChat.StartWorker();

        // 2. Configure another chat component for structured data extraction
        extractorChat.model = sharedModel;
        extractorChat.systemPrompt = "Extract the key information from the user's text and provide it in JSON format.";
        // This one would likely use a grammar to enforce JSON output.
        extractorChat.StartWorker();

        // Now you can use both for different tasks without loading two models!
        casualChat.Say("Can you tell me about your capabilities?");
        extractorChat.Say("My name is Jane Doe and my email is jane@example.com.");
    }
    ```

**Memory savings:** Instead of loading multiple models, you load one and share it. Much more efficient!  
