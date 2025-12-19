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

We normally use [bge-small-en-v1.5-q8_0.gguf](https://huggingface.co/CompendiumLabs/bge-small-en-v1.5-gguf/resolve/main/bge-small-en-v1.5-q8_0.gguf).


## Practical Example: Quest & Reputation System

A good way to visualize the practicality of embeddings is through an example. 
In this example we will guide you through how to make a quest trigger or lowering the user's reputation based on what they say.

We'll build it step by step, but for the impatient; The complete script is copyable in the bottom of the page.

### Step 1: Set up your basic structure and variables

The first step is to setup our components. We will add some statements for quests and some for hostile behavior - these are not exhaustive lists. 

**Do note** that it will take a longer time to embed a lot of sentences (depending on model and hardware of course), so depending on how complex your statements need to be, 
you might be better off having a handful and tuning the sensitivity of the trigger instead.

First, create your script that extends `NobodyWhoEncoder` and define your statement categories:

```gdscript
extends NobodyWhoEncoder

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

### Step 2: Initialize the embedding system


Set up the embedding model and start the worker:

```gdscript
func _ready():
    # Create and configure the embedding model
    var embedding_model = NobodyWhoModel.new()
    embedding_model.model_path = "res://models/bge-small-en-v1.5-q8_0.gguf"
    get_parent().add_child(embedding_model)
    
    # Link to the embedding model
    self.model_node = embedding_model
    self.encoding_finished.connect(_on_encoding_finished)
    self.start_worker()
    
    # Pre-generate embeddings for all statement types
    precompute_all_embeddings()
```


### Step 3: Precompute reference embeddings

Generate embeddings for all your reference statements:

```gdscript
func precompute_all_embeddings():
    # Generate embeddings for helpful statements
    for statement in quest_triggers:
        encode(statement)
        var embedding = await self.encoding_finished
        helpful_embeddings.append(embedding)

    # Generate embeddings for hostile statements
    for statement in hostile_statements:
        encode(statement)
        var embedding = await self.encoding_finished
        hostile_embeddings.append(embedding)
```


### Step 4: Add input handling for testing


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

### Step 5: Analyze player statements


Compare the player's message against your reference embeddings:

```gdscript
func analyze_player_statement(player_text: String):
    # Generate embedding for player input
    encode(player_text)
    var player_embedding = await self.encoding_finished
    
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

### Step 6: Handle the results


Trigger appropriate game systems based on detected intent:

```gdscript
func handle_helpful_information(text: String):
    # Trigger game systems based on detected intent
    print("🐉 Triggering quest: 'Audience with the Ancient Dragon'!")

func handle_hostile_intent(text: String):
    player_reputation -= 15
    print("Player expressed hostile intent! Reputation -15 (now: ", player_reputation, ")")
```
