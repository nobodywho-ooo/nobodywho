# Embeddings & RAG
_Using embeddings for semantic text comparison and retrieval-augmented generation._

---

## Understanding Text with Embeddings

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

### Download an Embedding Model

Embedding models are different from chat models. You need a model specifically trained for embeddings.

We normally use [bge-small-en-v1.5-q8_0.gguf](https://huggingface.co/CompendiumLabs/bge-small-en-v1.5-gguf/resolve/main/bge-small-en-v1.5-q8_0.gguf).


### Practical Example: Quest & Reputation System

A good way to visualize the practicality of embeddings is through an example.
In this example we will guide you through how to make a quest trigger or lowering the user's reputation based on what they say.

We'll build it step by step, but for the impatient; The complete script is copyable in the bottom of the page.

#### Step 1: Set up your basic structure and variables

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

#### Step 2: Initialize the embedding system


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


#### Step 3: Precompute reference embeddings

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


#### Step 4: Add input handling for testing


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

#### Step 5: Analyze player statements


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

#### Step 6: Handle the results


Trigger appropriate game systems based on detected intent:

```gdscript
func handle_helpful_information(text: String):
    # Trigger game systems based on detected intent
    print("Triggering quest: 'Audience with the Ancient Dragon'!")

func handle_hostile_intent(text: String):
    player_reputation -= 15
    print("Player expressed hostile intent! Reputation -15 (now: ", player_reputation, ")")
```

---

## Adding Long-Term Memory (RAG)

Great! You've got chat and embeddings working. Now let's add something useful: the ability to look up specific lore, dialogues, questlines etc.

### Why Your Game Needs Smart Document Search

Picture this: Your player is 40 hours into your RPG and asks an npc "Where do I find that crystal for the sword upgrade?"
Your LLM, without reranking, might give a generic answer or worse - make something up - leading to a bad player experience.
There are several ways to combat this, one is to load a lot of information into the context (i.e. the system prompt) but with a limited context, it might 'forget' the important information
or be confused by too much information. Instead we want to add a "long term memory" module to our language model.

To do this in the llm space you are going to use RAG (retrieval augmented generation) we are enriching the knowledge of the LLM by allowing it to search through a database of info we fed it.
There are many ways to do this. In NobodyWho we currently expose two major ways, one is embeddings; converting a sentence to a vector and then find the vectors that are closest to it.
This is powerful as you can save the vectors to a database or a file beforehand and then use the really fast and cheap cosine similarity to compare them. Another more expensive but more accurate way is to use a cross-encoder that figures out the relationship between the question and the document rather that just how similar they are.

This approach is often called reranking, due to how it is used as a step two, for sorting and filtering large knowledge databases accessed by LLMs. We'll call it ranking as we are working with a small enough dataset that we do not need a first pass to filter out irrelevant info.

Take this example:

```
Query: "Where do I find crystals for my sword upgrade?"
Documents: [
           "You asked the blacksmith: Where do I find crystals for my sword upgrade?",
           "The blacksmith said: Magic crystals are found in the Northern Mountains.",
           "You heard in the tavern: Magic crystals are not found in the Southern Desert."
]
```

If we rely just on comparing the query with the embeddings using cosine similarity (as we did with the embeddings), we will get back the document "You asked the blacksmith: Where do I find crystals for my sword upgrade?" as it is the most similar sentence to our query. This gave us no useful information and we have just wasted valuable context.

But with ranking, the cross-encoder model has been trained on knowing that the answer to the question is not the question itself, and thus ranks the document "The blacksmith said: Magic crystals are found in the Northern Mountains." the highest.


Here are the key terms you'll need:

| Term | Meaning |
| ---- | ------- |
| **Document Ranking** | Sorting text documents by how well they match or answer a question. |
| **RAG (Retrieval-Augmented Generation)** | A system that finds relevant documents first, then uses them to generate better LLM responses. |
| **Cross-encoder** | The type of model used for reranking - it reads both the query and document together to score relevance. |




Let's show you how to build smart search systems for your game.

### Download a Reranker Model

Reranking models are different from chat and embedding models. You need one specifically trained for document ranking.

We recommend [bge-reranker-v2-m3-Q8_0.gguf](https://huggingface.co/gpustack/bge-reranker-v2-m3-GGUF/resolve/main/bge-reranker-v2-m3-Q8_0.gguf) - it works well for most games and supports multiple languages.

Note that the current qwen3 reranker does not work, due to how they created the template as it has some missing fields.

### Practical Example: Smart NPC with Knowledge Base

Let's build a tavern keeper NPC that can answer player questions by searching through their personal knowledge. This NPC knows about the local area, quests, and rumors - perfect for creating more immersive and helpful characters.

We'll build it step by step, but for the impatient - the complete script is at the bottom.

#### Step 1: Set up your NPC's knowledge base

First, let's create a knowledge base for our tavern keeper - everything this specific NPC would realistically know:


    ```gdscript
    extends NobodyWhoChat

    @onready var reranker = $"../Rerank"
    @onready var chat_model = $"../ChatModel"

    # The tavern keeper's knowledge - ~50 pieces of local information way more than could fit in a standard 4096 sized context.
    var tavern_keeper_knowledge = PackedStringArray([
        "The lake contains a special clay that blacksmiths use to forge superior weapons.",
        "Ancient oak trees in the sacred grove provide wood that naturally resists dark magic.",
        "Silver veins run through the mountain caves, valuable for crafting blessed weapons.",
        "Rare moonflowers bloom in the ruins only once per season and have powerful magical properties.",
        "The mill pond contains perfect stones for sharpening blades to razor sharpness.",
        "Wild honey from forest bees makes potions more potent when used as a base ingredient.",
        "A hooded stranger was seen asking questions about the old castle ruins last week.",
        "Someone has been leaving fresh flowers at the grave of the village's first mayor.",
        "Strange animal tracks were found near the well that don't match any known creature.",
        "The church bell rang by itself three nights ago at exactly midnight.",
        "Farmers found crop circles in their wheat fields after the last thunderstorm.",
        "A merchant claims he saw lights moving through the abandoned mine from the hill road.",
        "Children report hearing music coming from the forest when they play near the edge of town.",
        "The weather has been unusually warm this winter, and the old-timers are worried.",
        "Someone broke into the general store but only stole a map of the local cave systems.",
        "A wolf with unusual blue eyes has been spotted watching the town from the tree line.",
        "Old Sarah runs the bakery and makes the best apple pies in three kingdoms. Her grandson Tom went missing last week.",
        "Blacksmith Gareth is always looking for quality iron ore and magic crystals. He pays double for rare materials.",
        "Merchant Elena travels between towns selling exotic spices and silk. She arrives every second Tuesday.",
        "Father Benedict runs the small chapel and knows ancient blessings that can ward off evil spirits.",
        "Widow Martha owns the general store and knows every piece of gossip in town within hours.",
        "Young apprentice Jake works for the blacksmith but dreams of becoming an adventurer himself.",
        "Doctor Thorne treats injuries and illnesses. He keeps rare healing herbs in his back garden.",
        "Stable master Owen knows every horse in the region and can track animals through the wilderness.",
        "Mayor Thompson inherited his position from his father and struggles with the town's growing problems.",
        "The old mine north of town has been abandoned for years. Strange sounds echo from deep inside at night.",
        "The forest path to the east is safe during the day, but wolves hunt there after sunset.",
        "Crystal Mines to the south produce valuable gems but have become dangerous recently.",
        "The ancient stone bridge over Miller's Creek was built by dwarves centuries ago and still stands strong.",
        "Darkwood Forest harbors bandits who prey on merchant caravans traveling the main road.",
        "The Whispering Caves get their name from the wind that creates eerie sounds through the rock formations.",
        "Lake Serenity freezes solid in winter, making it possible to cross on foot to the northern settlements.",
        "The old watchtower on Crow's Hill offers a view of the entire valley but hasn't been manned in decades.",
        "Sacred Grove is where the druids once practiced their rituals before they disappeared from the region.",
        "The ruins of Castle Blackrock still stand on the mountain, though none dare venture there anymore.",
        "Trader Gareth's caravan was attacked by bandits hiding somewhere in Darkwood Forest.",
        "Tom the baker's grandson disappeared near the Crystal Mines while collecting rare stones.",
        "Strange lights have been appearing in the Whispering Caves during moonless nights.",
        "Farmers report their livestock going missing near the edge of Darkwood Forest.",
        "The old mill wheel stopped working after something large damaged it upstream.",
        "Merchants complain about increased bandit activity on the eastern trade route.",
        "Several townsfolk have reported seeing ghostly figures near the abandoned mine at midnight.",
        "The village well's water tastes strange since the earthquake last month.",
        "Wild animals have been acting aggressively and fleeing deeper into the mountains.",
        "Ancient runes appeared overnight on the sacred standing stones outside town.",
        "The town was founded by refugees fleeing the Great Dragon War three hundred years ago.",
        "Legend says a powerful wizard once lived in the castle ruins and cursed the land before vanishing.",
        "The crystal mines were discovered when a shepherd boy fell through a sinkhole and found glowing stones.",
        "Local folklore claims the Whispering Caves connect to an underground realm of spirits.",
        "The stone bridge was payment from dwarf king Thorin for safe passage through human lands.",
        "Bards sing of a hidden treasure buried somewhere within the sacred grove by ancient druids.",
        "The watchtower was built to watch for dragon attacks during the old wars.",
        "Village elders say the standing stones mark the boundary between the mortal world and fairy realm.",
        "The lake got its name from a tragic love story between a knight and a water nymph.",
        "Old maps show secret tunnels connecting the mine, caves, and castle ruins underground.",
        "Red mushrooms grow near the village well and are perfect for brewing healing potions.",
        "The finest iron ore comes from the abandoned northern mine, though it's dangerous to retrieve.",
        "Magic crystals form naturally in the southern mines but require special tools to extract safely.",
        "Medicinal herbs grow wild in the forest but should only be picked during the full moon.",
    ])

    var ranked_docs = []
    ```

#### Step 2: Configure your components


    ```gdscript

    func _ready():
        # Set up the chat for generating helpful responses
        self.model_node = chat_model
        reranker.connect("ranking_finished", func(result): ranked_docs = result)
        reranker.start_worker()

        self.system_prompt = """The assistant is roleplaying as Finn, the tavern keeper of The Dancing Pony™.

        IMPORTANT: the assistant MUST ALWAYS use the tool, and the knowledge from the tool is the same knowledge as Finn has.
        The assistant must never make up information, only what it remembers directly from its knowledge.
        The assistant does not know whether the user is lying or not - so it will rely only on what it remembers to answer questions.
        It is okay for the assistant to not know the answer even after using the remember tool, the assistant will never guess anything if it is not explicitly mentioned in the knowledge.

        The assistant must always speak like a tavern keeper.

        """
        # Add the tool to remember stuff
        self.add_tool(remember, "The assistant can use this tool to remember its limited knowledge about the ingame world.")
        self.connect("response_finished", func(response: String): print("Finn says: ", response))
        start_worker()
    ```


#### Step 3: Set up a simple input system


    ```gdscript
    func _process(delta):
        if Input.is_action_just_pressed("enter"):
            var test_question = "Where is strider?"
            print("Player asks Finn: ", test_question)
            ask(test_question)
    ```


#### Step 4: Use ranked results to generate smart answers


    ```gdscript


    func remember(query: String) -> String:
        print("Finn searches his memory for: ", query)
        ranked_docs = "\n".join(reranker.rank_sync(query, tavern_keeper_knowledge, 3))
        print("Finn found: ", ranked_docs)

        return ranked_docs

    func _ready():
        ....
        # Add the tool to the llm to allow it to search for the most relevant lore.
        self.add_tool(remember, "The assistant can use this tool to remember its limited knowledge about the ingame world.")
        self.connect("response_finished", func(response: String): print("Finn says: ", response))
        start_worker()

    ```

When we run this we will get the response:

>Finn says: &lt;think&gt;
>Okay, let's see. The user asked about trading in the area.

```
Finn searches his memory for: What is trading like in the area?
.... goes through and ranks all the documents
Finn found:
Merchants complain about increased bandit activity on the eastern trade route.
Merchant Elena travels between towns selling exotic spices and silk. She arrives every second Tuesday.
Trader Gareth's caravan was attacked by bandits hiding somewhere in Darkwood Forest.
```

> From the tool response, I remember a few key points. First, merchants are complaining about bandits on the eastern trade route. That's a problem for traders. Then there's Merchant Elena who sells spices and silk every second Tuesday. That's a specific thing to mention. Also, Trader Gareth's caravan was attacked in Darkwood Forest. So the answer should include these points.
>
>I need to present this information clearly. Start by mentioning the bandit issue, then note Elena's schedule, and mention Gareth's attack. Make sure it's in a friendly, tavern keeper style. Check if there's anything else, but the tool response only gave these three points. Don't add anything extra. Alright, structure the response with those details.
>&lt;/think&gt;

and when good ole Finn is done thinking:

>Trading in the area is... if you'll forgive the blunt truth, *dangerous*. Merchants swear the eastern trade route is littered with bandits, and last week a caravan was ambushed in Darkwood Forest. But there are still opportunities! Merchant Elena brings rare spices and silk every second Tuesday—she's a reliable seller. Just be wary of the roads. And if you spot a caravan with a single rider, don't engage. They're probably bandits.


---

### Complete Scripts

<details>
<summary>Complete Godot Script (Click to expand)</summary>

```gdscript
extends NobodyWhoChat

@onready var reranker = $"../Rerank"
@onready var chat_model = $"../ChatModel"

# The tavern keeper's knowledge - ~50 pieces of local information way more than could fit in a standard 4096 sized context.
var tavern_keeper_knowledge = PackedStringArray([
    "The lake contains a special clay that blacksmiths use to forge superior weapons.",
    "Ancient oak trees in the sacred grove provide wood that naturally resists dark magic.",
    "Silver veins run through the mountain caves, valuable for crafting blessed weapons.",
    "Rare moonflowers bloom in the ruins only once per season and have powerful magical properties.",
    "The mill pond contains perfect stones for sharpening blades to razor sharpness.",
    "Wild honey from forest bees makes potions more potent when used as a base ingredient.",
    "A hooded stranger was seen asking questions about the old castle ruins last week.",
    "Someone has been leaving fresh flowers at the grave of the village's first mayor.",
    "Strange animal tracks were found near the well that don't match any known creature.",
    "The church bell rang by itself three nights ago at exactly midnight.",
    "Farmers found crop circles in their wheat fields after the last thunderstorm.",
    "A merchant claims he saw lights moving through the abandoned mine from the hill road.",
    "Children report hearing music coming from the forest when they play near the edge of town.",
    "The weather has been unusually warm this winter, and the old-timers are worried.",
    "Someone broke into the general store but only stole a map of the local cave systems.",
    "A wolf with unusual blue eyes has been spotted watching the town from the tree line.",
    "Old Sarah runs the bakery and makes the best apple pies in three kingdoms. Her grandson Tom went missing last week.",
    "Blacksmith Gareth is always looking for quality iron ore and magic crystals. He pays double for rare materials.",
    "Merchant Elena travels between towns selling exotic spices and silk. She arrives every second Tuesday.",
    "Father Benedict runs the small chapel and knows ancient blessings that can ward off evil spirits.",
    "Widow Martha owns the general store and knows every piece of gossip in town within hours.",
    "Young apprentice Jake works for the blacksmith but dreams of becoming an adventurer himself.",
    "Doctor Thorne treats injuries and illnesses. He keeps rare healing herbs in his back garden.",
    "Stable master Owen knows every horse in the region and can track animals through the wilderness.",
    "Mayor Thompson inherited his position from his father and struggles with the town's growing problems.",
    "The old mine north of town has been abandoned for years. Strange sounds echo from deep inside at night.",
    "The forest path to the east is safe during the day, but wolves hunt there after sunset.",
    "Crystal Mines to the south produce valuable gems but have become dangerous recently.",
    "The ancient stone bridge over Miller's Creek was built by dwarves centuries ago and still stands strong.",
    "Darkwood Forest harbors bandits who prey on merchant caravans traveling the main road.",
    "The Whispering Caves get their name from the wind that creates eerie sounds through the rock formations.",
    "Lake Serenity freezes solid in winter, making it possible to cross on foot to the northern settlements.",
    "The old watchtower on Crow's Hill offers a view of the entire valley but hasn't been manned in decades.",
    "Sacred Grove is where the druids once practiced their rituals before they disappeared from the region.",
    "The ruins of Castle Blackrock still stand on the mountain, though none dare venture there anymore.",
    "Trader Gareth's caravan was attacked by bandits hiding somewhere in Darkwood Forest.",
    "Tom the baker's grandson disappeared near the Crystal Mines while collecting rare stones.",
    "Strange lights have been appearing in the Whispering Caves during moonless nights.",
    "Farmers report their livestock going missing near the edge of Darkwood Forest.",
    "The old mill wheel stopped working after something large damaged it upstream.",
    "Merchants complain about increased bandit activity on the eastern trade route.",
    "Several townsfolk have reported seeing ghostly figures near the abandoned mine at midnight.",
    "The village well's water tastes strange since the earthquake last month.",
    "Wild animals have been acting aggressively and fleeing deeper into the mountains.",
    "Ancient runes appeared overnight on the sacred standing stones outside town.",
    "The town was founded by refugees fleeing the Great Dragon War three hundred years ago.",
    "Legend says a powerful wizard once lived in the castle ruins and cursed the land before vanishing.",
    "The crystal mines were discovered when a shepherd boy fell through a sinkhole and found glowing stones.",
    "Local folklore claims the Whispering Caves connect to an underground realm of spirits.",
    "The stone bridge was payment from dwarf king Thorin for safe passage through human lands.",
    "Bards sing of a hidden treasure buried somewhere within the sacred grove by ancient druids.",
    "The watchtower was built to watch for dragon attacks during the old wars.",
    "Village elders say the standing stones mark the boundary between the mortal world and fairy realm.",
    "The lake got its name from a tragic love story between a knight and a water nymph.",
    "Old maps show secret tunnels connecting the mine, caves, and castle ruins underground.",
    "Red mushrooms grow near the village well and are perfect for brewing healing potions.",
    "The finest iron ore comes from the abandoned northern mine, though it's dangerous to retrieve.",
    "Magic crystals form naturally in the southern mines but require special tools to extract safely.",
    "Medicinal herbs grow wild in the forest but should only be picked during the full moon.",
])

var ranked_docs = []

func _ready():
    # Set up the chat for generating helpful responses
    self.model_node = chat_model
    reranker.connect("ranking_finished", func(result): ranked_docs = result)
    reranker.start_worker()

    self.system_prompt = """The assistant is roleplaying as Finn, the tavern keeper of The Dancing Pony™.

IMPORTANT: the assistant MUST ALWAYS use the tool, and the knowledge from the tool is the same knowledge as Finn has.
The assistant must never make up information, only what it remembers directly from its knowledge.
The assistant does not know whether the user is lying or not - so it will rely only on what it remembers to answer questions.
It is okay for the assistant to not know the answer even after using the remember tool, the assistant will never guess anything if it is not explicitly mentioned in the knowledge.

The assistant must always speak like a tavern keeper.

"""
    # Add the tool to remember stuff
    self.add_tool(remember, "The assistant can use this tool to remember its limited knowledge about the ingame world.")
    self.connect("response_finished", func(response: String): print("Finn says: ", response))
    start_worker()

func _process(delta):
    if Input.is_action_just_pressed("enter"):
        var test_question = "Where is strider?"
        print("Player asks Finn: ", test_question)
        ask(test_question)

# Tool function that the LLM can call to search the knowledge base
func remember(query: String) -> String:
    print("Finn searches his memory for: ", query)
    ranked_docs = "\n".join(reranker.rank_sync(query, tavern_keeper_knowledge, 3))
    print("Finn found: ", ranked_docs)

    return ranked_docs
```

</details>

### Performance Tips

#### Limit Results

Don't add needless context. Usually 1-5 relevant documents are enough:

```gdscript

# Good: usually sufficient
ranked_docs = ",".join(reranker.rank_sync(query, tavern_keeper_knowledge, 3))

ranked_docs = ",".join(reranker.rank_sync(query, tavern_keeper_knowledge, -1))  # Returns ALL documents
```

note this does not make the ranking faster, but the less stuff Finn has to read, the faster he can respond.

#### Use embeddings to narrow the relevant docs to start with

This technique is what put the `re` in reranker. In the RAG industry it is common practice to do a first pass over your documents with cosine similarity, and thus narrowing the amount of results you have to process each time. This makes it feasible to have databases with millions of entries and not worry too much about performance.

depending on the specs you are going for I would not recommend ranking more than 100 results at a time.


## What's Next?

Now you can build smart search systems for your game! check out:

- **[Tool Calling](tool-calling.md)** for letting the LLM trigger game actions
