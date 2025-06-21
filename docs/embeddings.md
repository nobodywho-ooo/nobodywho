# Understanding Text with Embeddings
_A complete guide to using embeddings for semantic text comparison and natural language understanding._

---

Cool, you've got the basics of chat working! Now let's explore embeddings, which let you understand what text means rather than just matching exact words.

Embeddings are like a smart way to measure how similar two pieces of text are, even if they use completely different words. 
Instead of looking for exact matches, embeddings understand meaning.   
For example, "Hand me the red potion" and "Give me the scarlet flask" would be recognized as very similar, even though they share no common words.

Here are the key terms for working with embeddings:

| Term | Meaning |
| ---- | ------- |
| **Embedding Model (GGUF)** | A specialized `*.gguf` file trained to convert text into numerical vectors that represent meaning. |
| **Embedding** | A list of numbers (vector) that represents the meaning of a piece of text. |
| **Cosine Similarity** | A mathematical way to compare how similar two embeddings are, returning a value between 0 (completely different) and 1 (identical meaning). |
| **Semantic Search** | Finding text that means the same thing, even if the words are different. |
| **Vector** | The array of numbers that represents your text's meaning. |

Let's show you how to use embeddings to understand what your players really mean when they type commands.

## Download an Embedding Model

Embedding models are different from chat models. You need a model specifically trained for embeddings.

We normally use [bge-small-en-v1.5-q8_0.gguf](https://huggingface.co/CompendiumLabs/bge-small-en-v1.5-gguf/resolve/main/bge-small-en-v1.5-q8_0.gguf), but it might be a bit outdated.


## Practical Example: Quest & Reputation System

A good way to visualize the practicality of embeddings is through an example. 
In this example we will guide you through how to make a quest trigger or lowering the users reputation based on what they say.

We'll build it step by step, but for the impatient; The complete script is copyable in the bottom of the page.

### Step 1: Set up your basic structure and variables

The first step is to setup our components. We will add some statements for quests and some for hostile behavior - these are not exhaustive lists. 

**Do note** that it will take a longer time to embed a lot of sentences (depending on model and hardware of course), so depending on how complex your statements need to be, 
you might be better off having a handful and tuning the sensitivity of the trigger instead.

=== ":simple-godotengine: Godot"

    First, create your script that extends `NobodyWhoEmbedding` and define your statement categories:

    ```gdscript
    extends NobodyWhoEmbedding

    var quest_triggers= [
        "I know where the dragon rests",
        "The druid told me the proper way to meet the dragon",
        "I discovered the ritual needed to gain the dragon's audience",
        "I know about the sacred grove"
    ]
    
    var hostile_statements = [
        "I want to kill the dragon",
        "I'm going to destroy everything",
        "I hate this place and everyone in it",
        "I will burn down the village",
        "Everyone here deserves to die"
    ]

    var helpful_embeddings = []
    var hostile_embeddings = []
    var player_reputation = 0
    ```

=== ":simple-unity: Unity"

    Initialize the model and embedding components:

    ```csharp
    using System.Collections.Generic;
    using UnityEngine;
    using NobodyWho;
    using System.IO;

    public class QuestReputationSystem : MonoBehaviour
    {
        private Model model;
        private Embedding embedding;
        
        private string[] questTriggers = {
            "I know where the dragon rests",
            "The druid told me the proper way to meet the dragon",
            "I discovered the ritual needed to gain the dragon's audience",
            "I know about the sacred grove"
        };
        
        private string[] hostileStatements = {
            "I want to kill the dragon",
            "I'm going to destroy everything",
            "I hate this place and everyone in it",
            "I will burn down the village",
            "Everyone here deserves to die"
        };
        
        private List<float[]> helpfulEmbeddings = new List<float[]>();
        private List<float[]> hostileEmbeddings = new List<float[]>();
        
        private int precomputeIndex = 0;
        private int currentCategory = 0; // 0=helpful, 1=hostile
        private bool isPrecomputing = true;
        private int playerReputation = 0;

        void Start()
        {
            // Create model component
            model = gameObject.AddComponent<Model>();
            string modelPath = Path.Combine(Application.streamingAssetsPath, "bge-small-en-v1.5-q8_0.gguf");
            model.modelPath = modelPath;
            
            // Create and configure embedding component
            embedding = gameObject.AddComponent<Embedding>();
            embedding.model = model;
            embedding.StartWorker();
            embedding.onEmbeddingComplete.AddListener(OnEmbeddingComplete);
            
            // Start precomputing all statement embeddings
            PrecomputeAllEmbeddings();
        }
    ```

### Step 2: Initialize the embedding system

=== ":simple-godotengine: Godot"

    Set up the embedding model and start the worker:

    ```gdscript
    func _ready():
        # Create and configure the embedding model
        var embedding_model = NobodyWhoModel.new()
        embedding_model.model_path = "res://models/bge-small-en-v1.5-q8_0.gguf"
        get_parent().add_child(embedding_model)
        
        # Link to the embedding model
        self.model_node = embedding_model
        self.embedding_finished.connect(_on_embedding_finished)
        self.start_worker()
        
        # Pre-generate embeddings for all statement types
        precompute_all_embeddings()
    ```

=== ":simple-unity: Unity"

    Initialize the model and embedding components:

    ```csharp
    void Start()
    {
        // Create model component
        model = gameObject.AddComponent<Model>();
        string modelPath = Path.Combine(Application.streamingAssetsPath, "bge-small-en-v1.5-q8_0.gguf");
        model.modelPath = modelPath;
        
        // Create and configure embedding component
        embedding = gameObject.AddComponent<Embedding>();
        embedding.model = model;
        embedding.StartWorker();
        embedding.onEmbeddingComplete.AddListener(OnEmbeddingComplete);
        
        // Start precomputing all statement embeddings
        PrecomputeAllEmbeddings();
    }
    ```

### Step 3: Precompute reference embeddings

=== ":simple-godotengine: Godot"

    Generate embeddings for all your reference statements:

    ```gdscript
    func precompute_all_embeddings():
        # Generate embeddings for helpful statements
        for statement in quest_triggers:
            embed(statement)
            var embedding = await self.embedding_finished
            helpful_embeddings.append(embedding)
        
        # Generate embeddings for hostile statements
        for statement in hostile_statements:
            embed(statement)
            var embedding = await self.embedding_finished
            hostile_embeddings.append(embedding)
    ```

=== ":simple-unity: Unity"

    Here's where Unity gets a bit more complex than Godot. Since Unity uses callbacks instead of async/await, 
    we can't just wait for each embedding to finish before starting the next one. 
    Instead, we need to track our progress through all the statements. ( Do not worry we are planning to have async/await in Unity down the road)

    **Why do we need this complexity?** A model can only process one text at a time, 
    and we have two different categories of statements to process. Without proper tracking, our embeddings would get mixed up 
    - the vector value of a hostile statement might end up in the helpful category, or we might try to analyze 
    player input before we've finished setting up our reference embeddings.


    ```csharp
    // When embed is finished it will call the onEmbeddingComplete event, which we hooked up to the method of the same name in Step 2
    void PrecomputeAllEmbeddings()
    {
        // Start with helpful statements (category 0)
        if (currentCategory == 0 && precomputeIndex < questTriggers.Length)
        {
            embedding.Embed(questTriggers[precomputeIndex]);
        }
        // Then move to hostile statements (category 1)
        else if (currentCategory == 1 && precomputeIndex < hostileStatements.Length)
        {
            embedding.Embed(hostileStatements[precomputeIndex]);
        }
        }

        void OnEmbeddingComplete(float[] embeddingResult)
        {
        if (isPrecomputing)
        {
            // Store embedding in appropriate category
            if (currentCategory == 0)
            {
                helpfulEmbeddings.Add(embeddingResult);
            }
            else if (currentCategory == 1)
            {
                hostileEmbeddings.Add(embeddingResult);
            }
            
            precomputeIndex++;
            
            // Move to next category when current is complete
            if ((currentCategory == 0 && precomputeIndex >= questTriggers.Length) ||
                (currentCategory == 1 && precomputeIndex >= hostileStatements.Length))
            {
                currentCategory++;
                precomputeIndex = 0;
            }
            
            // Continue precomputing or finish
            if (currentCategory < 2)
            {
                PrecomputeAllEmbeddings();
            }
            else
            {
                isPrecomputing = false;
            }
        }
        else
        {
            // Process player input embedding
            AnalyzePlayerStatement(embeddingResult);
        }
    }



    ```

    This tracking system ensures each embedding gets stored in the right category and we know when we're done with setup.

### Step 4: Add input handling for testing

=== ":simple-godotengine: Godot"

    Add a simple test trigger using the enter key:

    ```gdscript
    func _input(event):
        # Handle enter key press to send hardcoded test message
        if event is InputEventKey and event.pressed:
            if event.keycode == KEY_ENTER:
                var test_message = "I know the location of the dragon"
                print("Sending test message: ", test_message)
                analyze_player_statement(test_message)
    ```

=== ":simple-unity: Unity"

    Use Update() to check for enter key presses:

    ```csharp
    void Update()
    {
        // Handle enter key press to send hardcoded test message
        if (Input.GetKeyDown(KeyCode.Return) || Input.GetKeyDown(KeyCode.KeypadEnter))
        {
            string testMessage = "I learned the location of the dragon";
            Debug.Log($"Sending test message: {testMessage}");
            ProcessPlayerStatement(testMessage);
        }
    }
    ```

### Step 5: Analyze player statements

=== ":simple-godotengine: Godot"

    Compare the player's message against your reference embeddings:

    ```gdscript
    func analyze_player_statement(player_text: String):
        # Generate embedding for player input
        embed(player_text)
        var player_embedding = await self.embedding_finished
        
        # Compare against both categories
        var best_helpful_similarity = get_best_similarity(player_embedding, helpful_embeddings)
        var best_hostile_similarity = get_best_similarity(player_embedding, hostile_embeddings)
        
        print("Helpful similarity: ", best_helpful_similarity)
        print("Hostile similarity: ", best_hostile_similarity)
        
        # Use similarity threshold of 0.8 and compare categories
        if best_helpful_similarity > 0.8 and best_helpful_similarity > best_hostile_similarity:
            handle_helpful_information(player_text)
        elif best_hostile_similarity > 0.8 and best_hostile_similarity > best_helpful_similarity:
            handle_hostile_intent(player_text)
        else:
            print("Unclear intent - no strong match found")
    ```

=== ":simple-unity: Unity"

    Compare the player's message against your embeddings:

    ```csharp
    void AnalyzePlayerStatement(float[] playerEmbedding)
    {
        // Compare against both categories using CosineSimilarity
        float bestHelpfulSimilarity = GetBestSimilarity(playerEmbedding, helpfulEmbeddings);
        float bestHostileSimilarity = GetBestSimilarity(playerEmbedding, hostileEmbeddings);
        
        Debug.Log($"Helpful similarity: {bestHelpfulSimilarity}");
        Debug.Log($"Hostile similarity: {bestHostileSimilarity}");
        
        // Use similarity threshold of 0.8 and compare categories
        if (bestHelpfulSimilarity > 0.8f && bestHelpfulSimilarity > bestHostileSimilarity)
        {
            HandleHelpfulInformation(bestHelpfulSimilarity);
        }
        else if (bestHostileSimilarity > 0.8f && bestHostileSimilarity > bestHelpfulSimilarity)
        {
            HandleHostileIntent(bestHostileSimilarity);
        }
        else
        {
            Debug.Log("Unclear intent - no strong match found");
        }
    }
    ```

### Step 6: Handle the results

=== ":simple-godotengine: Godot"

    Trigger appropriate game systems based on detected intent:

    ```gdscript
    func handle_helpful_information(text: String):
        # Trigger game systems based on detected intent
        print("🐉 Triggering quest: 'Audience with the Ancient Dragon'!")

    func handle_hostile_intent(text: String):
        player_reputation -= 15
        print("Player expressed hostile intent! Reputation -15 (now: ", player_reputation, ")")
    ```

=== ":simple-unity: Unity"

    Trigger appropriate game systems based on detected intent:

    ```csharp
    void HandleHelpfulInformation(float confidence)
    {
        Debug.Log("🐉 Triggering quest: 'Audience with the Ancient Dragon'!");
    }

    void HandleHostileIntent(float confidence)
    {
        playerReputation -= 15;
        Debug.Log($"Player expressed hostile intent! Reputation -15 (now: {playerReputation})");
    }
    ```

---

### Complete Scripts

<details markdown>
<summary markdown>:simple-godotengine: Complete Godot Script (Click to expand)</summary>

```gdscript
extends NobodyWhoEmbedding

# Different types of player statements
var quest_triggers= [
    "I know where the dragon rests",
    "The druid told me the proper way to meet the dragon",
    "I discovered the ritual needed to gain the dragon's audience",
    "I know about the sacred grove"
]

var hostile_statements = [
    "I want to kill the dragon",
    "I'm going to destroy everything",
    "I hate this place and everyone in it",
    "I will burn down the village",
    "Everyone here deserves to die"
]

var helpful_embeddings = []
var hostile_embeddings = []
var player_reputation = 0

func _ready():
    # Link to the embedding model
    self.model_node = NobodyWhoModel.new()
    self.model_node.model_path = "res://models/bge-small-en-v1.5-q8_0.gguf"

    self.embedding_finished.connect(_on_embedding_finished)
    self.start_worker()
    
    # Pre-generate embeddings for all statement types
    precompute_all_embeddings()

func _input(event):
    # Handle enter key press to send hardcoded test message
    if event is InputEventKey and event.pressed:
        if event.keycode == KEY_ENTER:
            var test_message = "I know the location of the dragon"
            print("Sending test message: ", test_message)
            analyze_player_statement(test_message)

func precompute_all_embeddings():
    # Generate embeddings for helpful statements
    for statement in quest_triggers:
        embed(statement)
        var embedding = await self.embedding_finished
        helpful_embeddings.append(embedding)
    
    # Generate embeddings for hostile statements
    for statement in hostile_statements:
        embed(statement)
        var embedding = await self.embedding_finished
        hostile_embeddings.append(embedding)

func analyze_player_statement(player_text: String):
    # Generate embedding for player input
    embed(player_text)
    var player_embedding = await self.embedding_finished
    
    # Compare against both categories
    var best_helpful_similarity = get_best_similarity(player_embedding, helpful_embeddings)
    var best_hostile_similarity = get_best_similarity(player_embedding, hostile_embeddings)
    
    print("Helpful similarity: ", best_helpful_similarity)
    print("Hostile similarity: ", best_hostile_similarity)
    
    # Use similarity threshold of 0.8 and compare categories
    if best_helpful_similarity > 0.8 and best_helpful_similarity > best_hostile_similarity:
        handle_helpful_information(player_text)
    elif best_hostile_similarity > 0.8 and best_hostile_similarity > best_helpful_similarity:
        handle_hostile_intent(player_text)
    else:
        print("Unclear intent - no strong match found")

func get_best_similarity(player_embedding, statement_embeddings):
    # Find highest cosine similarity in the category
    var best_similarity = 0.0
    for embedding in statement_embeddings:
        var similarity = cosine_similarity(player_embedding, embedding)
        if similarity > best_similarity:
            best_similarity = similarity
    return best_similarity

func handle_helpful_information(text: String):
    # Trigger game systems based on detected intent
    print("🐉 Triggering quest: 'Audience with the Ancient Dragon'!")

func handle_hostile_intent(text: String):
    player_reputation -= 15
    print("Player expressed hostile intent! Reputation -15 (now: ", player_reputation, ")")

func _on_embedding_finished(embedding):
    # Signal callback for completed embeddings
    pass
```

</details>

<details markdown>
<summary markdown>:simple-unity: Complete Unity Script (Click to expand)</summary>

```csharp
using System.Collections.Generic;
using UnityEngine;
using NobodyWho;
using System.IO;

public class QuestReputationSystem : MonoBehaviour
{
    private Model model;
    private Embedding embedding;
    
    private string[] questTriggers = {
        "I know where the dragon rests",
        "The druid told me the proper way to meet the dragon",
        "I discovered the ritual needed to gain the dragon's audience",
        "I know about the sacred grove"
    };
    
    private string[] hostileStatements = {
        "I want to kill the dragon",
        "I'm going to destroy everything",
        "I hate this place and everyone in it",
        "I will burn down the village",
        "Everyone here deserves to die"
    };
    
    private List<float[]> helpfulEmbeddings = new List<float[]>();
    private List<float[]> hostileEmbeddings = new List<float[]>();
    
        private int precomputeIndex = 0;
    private int currentCategory = 0; // 0=helpful, 1=hostile
        private bool isPrecomputing = true;
    private int playerReputation = 0;

        void Start()
        {
        // Create model component
        model = gameObject.AddComponent<Model>();
        string modelPath = Path.Combine(Application.streamingAssetsPath, "bge-small-en-v1.5-q8_0.gguf");
        model.modelPath = modelPath;
        
        // Create and configure embedding component
        embedding = gameObject.AddComponent<Embedding>();
        embedding.model = model;
            embedding.StartWorker();
            embedding.onEmbeddingComplete.AddListener(OnEmbeddingComplete);
            
        // Start precomputing all statement embeddings
        PrecomputeAllEmbeddings();
    }

    void Update()
    {
        // Handle enter key press to send hardcoded test message
        if (Input.GetKeyDown(KeyCode.Return) || Input.GetKeyDown(KeyCode.KeypadEnter))
        {
            string testMessage = "I learned the location of the dragon";
            Debug.Log($"Sending test message: {testMessage}");
            ProcessPlayerStatement(testMessage);
        }
    }

    void PrecomputeAllEmbeddings()
    {
        // Start with helpful statements (category 0)
        if (currentCategory == 0 && precomputeIndex < questTriggers.Length)
        {
            embedding.Embed(questTriggers[precomputeIndex]);
        }
        // Then move to hostile statements (category 1)
        else if (currentCategory == 1 && precomputeIndex < hostileStatements.Length)
        {
            embedding.Embed(hostileStatements[precomputeIndex]);
            }
        }

        void OnEmbeddingComplete(float[] embeddingResult)
        {
            if (isPrecomputing)
            {
            // Store embedding in appropriate category
            if (currentCategory == 0)
            {
                helpfulEmbeddings.Add(embeddingResult);
            }
            else if (currentCategory == 1)
            {
                hostileEmbeddings.Add(embeddingResult);
            }
            
                precomputeIndex++;
                
            // Move to next category when current is complete
            if ((currentCategory == 0 && precomputeIndex >= questTriggers.Length) ||
                (currentCategory == 1 && precomputeIndex >= hostileStatements.Length))
                {
                currentCategory++;
                precomputeIndex = 0;
                }
            
            // Continue precomputing or finish
            if (currentCategory < 2)
                {
                PrecomputeAllEmbeddings();
            }
            else
            {
                isPrecomputing = false;
            }
        }
        else
        {
            // Process player input embedding
            AnalyzePlayerStatement(embeddingResult);
        }
    }

    public void ProcessPlayerStatement(string playerText)
    {
        // Only process if precomputation is complete
            if (!isPrecomputing)
            {
                embedding.Embed(playerText);
            }
        }

    void AnalyzePlayerStatement(float[] playerEmbedding)
    {
        // Compare against both categories using CosineSimilarity
        float bestHelpfulSimilarity = GetBestSimilarity(playerEmbedding, helpfulEmbeddings);
        float bestHostileSimilarity = GetBestSimilarity(playerEmbedding, hostileEmbeddings);
        
        Debug.Log($"Helpful similarity: {bestHelpfulSimilarity}");
        Debug.Log($"Hostile similarity: {bestHostileSimilarity}");
        
        // Use similarity threshold of 0.8 and compare categories
        if (bestHelpfulSimilarity > 0.8f && bestHelpfulSimilarity > bestHostileSimilarity)
        {
            HandleHelpfulInformation(bestHelpfulSimilarity);
        }
        else if (bestHostileSimilarity > 0.8f && bestHostileSimilarity > bestHelpfulSimilarity)
        {
            HandleHostileIntent(bestHostileSimilarity);
        }
        else
        {
            Debug.Log("Unclear intent - no strong match found");
        }
    }

    float GetBestSimilarity(float[] playerEmbedding, List<float[]> statementEmbeddings)
    {
        // Find highest cosine similarity in the category
        float bestSimilarity = 0f;
        foreach (float[] statementEmbedding in statementEmbeddings)
        {
            float similarity = embedding.CosineSimilarity(playerEmbedding, statementEmbedding);
            if (similarity > bestSimilarity)
            {
                bestSimilarity = similarity;
            }
        }
        return bestSimilarity;
    }

    void HandleHelpfulInformation(float confidence)
    {
        Debug.Log("🐉 Triggering quest: 'Audience with the Ancient Dragon'!");
    }

    void HandleHostileIntent(float confidence)
    {
        playerReputation -= 15;
        Debug.Log($"Player expressed hostile intent! Reputation -15 (now: {playerReputation})");
        }
    }
    ```

</details>

## Testing Your Embeddings

When you run your scene, you should see the embedding system working. The test will show that phrases about dragons have higher similarity to each other than to unrelated text.
Change the text in the update methods to see different results.

**What to expect:**

- In Godot, check the Output panel for similarity scores
- In Unity, check the Console window for debug messages
- Similar phrases should have similarity scores above 0.5
- Unrelated text should have much lower similarity scores

**If nothing happens:**

- Make sure you're using an embedding model, not a chat model
- Look for error messages in the console
- Start your editor through the command line and check the logs

**Understanding the numbers:**
 from 0 to 1
- 0.8+ means very similar meaning
- 0.5-0.8 means somewhat related
- Below 0.3 means probably unrelated

Now you can build smart text understanding into your program! Try experimenting with different phrases and models to see how the embeddings capture meaning beyond just matching words.
