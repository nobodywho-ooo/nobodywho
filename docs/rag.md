# Adding long term memory to your characters
_Build AI systems that can search through your game's lore, dialog, or knowledge base and find the most relevant information._

---

Great! You've got chat and embeddings working. Now lets add something useful: the ability to look up specific lore, dialogues, questlines etc.

## Why Your Game Needs Smart Document Search

Picture this: Your player is 40 hours into your RPG and asks an npc "Where do I find that crystal for the sword upgrade?" 
Your LLM, without reranking, might give a generic answer or worse - make something up - leading to a bad player experience. 
There are several ways to combat this, one is to load a lot of information into the context (i.e. the system prompt) but with a limited context, it might 'forget' the important information
or be confused by too much information. Instead we want to add a "long term memory" module to our language model.

To do this in the llm space you are going to use RAG (retreival augmented generation) we are enriching the knowledge of the LLM by allowing it to search through a database of info we fed it. 
There are many ways to do this. In Nobodywho we currently expose two major ways, one is embeddings; converting a sentence to a vector and then find the vectors that are closest to it.
This is powerful as you can save the vectors to a database or a file beforehand and then use the really fast and cheap cosine similarity to compare them. Another more expensive but more accurate way is to use a cross-encoder that figures out the relationship between the question and the document rather that just how similar they are. 

This approach is often called reranking, due to how it is used as a step two, for sorting and filtering large knowledge databases accesed by LLMs. I'll call it ranking as we are working with a small enough dataset that we do not need a first pass to filter out irrelevant info.

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

## Download a Reranker Model

Reranking models are different from chat and embedding models. You need one specifically trained for document ranking.

We recommend [bge-reranker-v2-m3-Q8_0.gguf](https://huggingface.co/gpustack/bge-reranker-v2-m3-GGUF/resolve/main/bge-reranker-v2-m3-Q8_0.gguf) - it works well for most games and supports multiple languages.

Note that the current qwen3 reranker does not work, due to how they created the template as it has some missing fields.

## Practical Example: Smart NPC with Knowledge Base

Let's build a tavern keeper NPC that can answer player questions by searching through their personal knowledge. This NPC knows about the local area, quests, and rumors - perfect for creating more immersive and helpful characters.

We'll build it step by step, but for the impatient - the complete script is at the bottom.

### Step 1: Set up your NPC's knowledge base

First, let's create a knowledge base for our tavern keeper - everything this specific NPC would realistically know:

=== ":simple-godotengine: Godot"

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

### Step 2: Configure your components

=== ":simple-godotengine: Godot"

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


### Step 3: Set up a simple input system

=== ":simple-godotengine: Godot"

    ```gdscript
    func _process(delta):
        if Input.is_action_just_pressed("enter"):
            var test_question = "Where is strider?"
            print("Player asks Finn: ", test_question)
            say(test_question)
    ```


### Step 4: Use ranked results to generate smart answers

=== ":simple-godotengine: Godot"

    ```gdscript
 
    
    func remember(query: String) -> String:
        print("🔍 Finn searches his memory for: ", query)
        ranked_docs = "\n".join(reranker.rank_sync(query, tavern_keeper_knowledge, 3))
        print("🔍 Finn found: ", ranked_docs)

        return ranked_docs

    func _ready():
        ....
        # Add the tool to the llm to allow it to search for the most relevant lore.
        self.add_tool(remember, "The assistant can use this tool to remember its limited knowledge about the ingame world.")
        self.connect("response_finished", func(response: String): print("Finn says: ", response))
        start_worker()

    ```

When we run this we will get the response:

>Finn says: <think>
>Okay, let's see. The user asked about trading in the area.

```
🔍 Finn searches his memory for: What is trading like in the area?
.... goes through and ranks all the documents
🔍 Finn found: 
Merchants complain about increased bandit activity on the eastern trade route.
Merchant Elena travels between towns selling exotic spices and silk. She arrives every second Tuesday.
Trader Gareth's caravan was attacked by bandits hiding somewhere in Darkwood Forest.
```

> From the tool response, I remember a few key points. First, merchants are complaining about bandits on the eastern trade route. That's a problem for traders. Then there's Merchant Elena who sells spices and silk every second Tuesday. That's a specific thing to mention. Also, Trader Gareth's caravan was attacked in Darkwood Forest. So the answer should include these points.
>
>I need to present this information clearly. Start by mentioning the bandit issue, then note Elena's schedule, and mention Gareth's attack. Make sure it's in a friendly, tavern keeper style. Check if there's anything else, but the tool response only gave these three points. Don't add anything extra. Alright, structure the response with those details.
></think>

and when good ole Finn is done thinking:

>Trading in the area is... if you'll forgive the blunt truth, *dangerous*. Merchants swear the eastern trade route is littered with bandits, and last week a caravan was ambushed in Darkwood Forest. But there are still opportunities! Merchant Elena brings rare spices and silk every second Tuesday—she’s a reliable seller. Just be wary of the roads. And if you spot a caravan with a single rider, don’t engage. They’re probably bandits.


---

### Complete Scripts

<details markdown>
<summary markdown>:simple-godotengine: Complete Godot Script (Click to expand)</summary>

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
        say(test_question)

# Tool function that the LLM can call to search the knowledge base
func remember(query: String) -> String:
    print("🔍 Finn searches his memory for: ", query)
    ranked_docs = "\n".join(reranker.rank_sync(query, tavern_keeper_knowledge, 3))
    print("🔍 Finn found: ", ranked_docs)

    return ranked_docs
```

</details>

## Performance Tips

### Limit Results

Don't add needless context. Usually 1-5 relevant documents are enough:

```gdscript

# Good: usually sufficient
ranked_docs = ",".join(reranker.rank_sync(query, tavern_keeper_knowledge, 3))

ranked_docs = ",".join(reranker.rank_sync(query, tavern_keeper_knowledge, -1))  # Returns ALL documents
```

note this does not make the ranking faster, but the less stuff Finn has to read, the faster he can respond.

### Use embeddings to narrow the relevant docs to start with

This technique is what put the `re` in reranker. In the RAG industry it is common practice to do a first pass over your documents with cosine similarity, and thus narrowing the amount of results you have to process each time. This makes it feasible to have databases with millions of entries and not worry too much about performance. 

depending on the specs you are going for I would not recommend ranking more than 100 results at a time.


# What's Next?

Now you can build smart search systems for your game! check out:

- **[Embeddings](embeddings.md)** for getting a better understanding of the basics
- **[Tool Calling](chat/tool-calling.md)** for letting the LLM trigger game actions
