# Structured Output
_Getting reliable, structured responses from your models_

---

Congratulations - you have understood the basics of having a large language models generate text for you. 
You are now ready for some more juicy and complex options.

Here are the key terms you should know:

| Term | Meaning |
| ---- | ------- |
| **GBNF** | GGML Backus-Naur Form - a way to define strict rules for output format |
| **Grammar** | The set of rules that define what valid output looks like |
| **Token** | A piece of text (word, punctuation, etc.) that the model generates, generally 1 to 4 characters long |
| **Encoder** | Translates text into tokens that the model can understand |


## My model is so stupid that it can not even write json

Yeah, most models will fail to generate valid json at some point if you just ask it to. 
But fret not dear friend, the solution you are looking for is called :star: **STRUCTURED OUTPUT** :star:. 

It is pretty much what it claims to be; A system that constrains the model's vocabulary to one that you determine.
This can be useful for a myriad of things, from forcing the LLM to never use modern words, to using the LLM
as the engine for your own procedural generation dungeon room.

This section will take you through creating your own grammar that the model will have to use.

### Why GBNF Beats Prompt Engineering

You've probably tried this before:

```
""" Please respond in JSON format with name, level, and class fields
Only use those fields.
Only use valid json.
All json attributes should have " around them.
Please do not deviate from the instructions.
You will lose 10 points if you use other fields than level, class and name.
Do not write a message just json.
If you do not respond in valid json I will lose my job and my kids will starve.
"""
```

And got back something like:
```
Sure! Here's a character: {"name": "Eldara", "level": 15, "class": Wizard} - hope this helps!
```

Notice the problems? Missing quotes around "Wizard", extra text before and after. Your JSON parser explodes. 💥

GBNF fixes this by making it **impossible** for the model to generate anything except the format you define:

```json
{"name": "Eldara", "level": 15, "class": "Wizard"}
```

Valid :clap: every :clap: time :clap:.

## Understanding GBNF Grammar Rules

### The Absolute Basics

A GBNF grammar is made up of **rules**. Each rule says "this thing can be made from these parts":

```
rule-name ::= what-it-can-be
```

### Your First Grammar: Hello World

Let's start with the simplest possible grammar:

```
root ::= "Hello World"
```

This says: "The output must be exactly the text 'Hello World'". That's it. The model can't say anything else.

Try this and the model will always output: `Hello World`

### Adding Choices with `|`

What if we want some variety? Use `|` (pipe) to give options:

```
root ::= "Hello World" | "Hi there" | "Greetings"
```

Now the model can choose between these three options, but nothing else.

### Building Blocks with Multiple Rules

Here's where it gets interesting. You can break things into smaller pieces:

```
root ::= greeting " " name
greeting ::= "Hello" | "Hi" | "Hey"
name ::= "World" | "Friend" | "There"
```

This creates outputs like:
- `Hello World`
- `Hi Friend`
- `Hey There`

The model picks one option from `greeting`, adds a space, then picks one option from `name`.

### Character Classes

Instead of listing every letter, use character classes:

```
root ::= letter letter letter
letter ::= [a-z]
```

`[a-z]` means "any lowercase letter from a to z". This generates random 3-letter combinations like `cat`, `how`, `dog`.
so letter letter letter will make a three letter word

Common character classes:
- `[a-z]` - lowercase letters
- `[A-Z]` - uppercase letters  
- `[0-9]` - digits
- `[a-zA-Z]` - any letter
- `[a-zA-Z0-9]` - letters and numbers


### Repetitions


This quickly becomes tedious if you want to create either long words or just any word. This is where repetitions come in:

- `*` means "zero or more"
- `+` means "one or more"  
- `?` means "optional (zero or one)"

- `{n}` means "exactly n times"
- `{n,}` means "at least n times"
- `{n,m}` means "at least n and at most m times"

```
root ::= letter+
```

This means "one or more lowercase letters" - so you get words like `hello`, `a`, `supercalifragilisticexpialidocious`.

```
root ::= [a-z]+ [0-9]*
```

This means "letters followed by optional numbers" - so you get `hello`, `test123`, `word`.


### Building JSON Step by Step

Now that you have been tricked into learning the basics of regex, we should build a small JSON generator. Start simple:

```
root ::= "{" "}"
```

This only generates: `{}`

Add one field:

```
root ::= "{" "\"name\"" ":" string "}"
string ::= "\"" [a-zA-Z]+ "\""
```

This generates: `{"name":"Bob"}` (where Bob is any sequence of letters)

Add more fields:

```
root ::= "{" "\"name\"" ":" string "," "\"level\"" ":" number "}"
string ::= "\"" [a-zA-Z]+ "\""
number ::= [0-9]+
```

This generates: `{"name":"Alice","level":"25"}`

### Making It Flexible

Use repetition to handle variable numbers of fields:

```
root ::= "{" pair ("," pair)* "}"
pair ::= word ":" word
word ::= "\"" [a-zA-Z]+ "\""
```

The `("," pair)*` means "zero or more additional pairs, each preceded by a comma". This generates:
- `{"name":"Bob"}`
- `{"name":"Alice","job":"Wizard"}`
- `{"name":"Charlie","job":"Knight","weapon":"Sword"}`

### Whitespace: Making It Readable

Add optional whitespace to make output prettier:

```
root ::= "{" ws pair (ws "," ws pair)* ws "}"
pair ::= string ws ":" ws string
string ::= "\"" [a-zA-Z ]+ "\""
ws ::= [ \t\n]*
```

The `ws` rule means "whitespace" - zero or more spaces, tabs, or newlines. Now you get nicely formatted JSON.

### Advanced: Specific Values

Control exactly what values are allowed:

```
root ::= "{" "\"class\"" ":" class-type "}"
class-type ::= "\"Warrior\"" | "\"Mage\"" | "\"Rogue\"" | "\"Cleric\""
```

This only allows those four specific classes - no hallucinated "Tank-operator" in your neolithic era game!

### Nested Structures

Build complex nested data:

```
root ::= "{" "\"character\"" ":" character-object "}"
character-object ::= "{" "\"name\"" ":" string "," "\"stats\"" ":" stats-object "}"
stats-object ::= "{" "\"hp\"" ":" number "," "\"mp\"" ":" number "}"
string ::= "\"" [a-zA-Z ]+ "\""
number ::= [0-9]+
```

This creates nested JSON like:
```json
{"character":{"name":"Gandalf","stats":{"hp":"100","mp":"200"}}}
```

## Performance Optimization: Compact Formats

Now that you understand GBNF with JSON, let's talk optimization. JSON is verbose and every token costs time. For high-performance applications, you can create much more compact formats.

### Why Compact Formats Matter

**JSON Format:**
```json
{"name":"Gandalf","level":15,"class":"Mage","hp":100,"mp":80}
```
*60 characters, ~38 tokens*

**Compact Format:**
```
Gandalf|High|Mage|Low|High
```
*22 characters, ~10 tokens*

**That's ~4 times faster while maintaining the same information!**

### Building Compact Formats

Start with pipe-separated values:

```
root ::= [A-Z][a-z]+ "|" [1-9][0-9]? "|" class-type
class-type ::= "Warrior" | "Mage" | "Rogue" | "Cleric"
```

This generates: `Gandalf|15|Mage` (semantically clear - no ambiguity about what "Mage" means!)

**Why not single letters?** If you used `"W" | "M" | "R" | "C"`, the LLM has no inherent knowledge that "M" means "Mage" rather than "Monk" or "Mercenary". The model generates tokens based on semantic understanding, not arbitrary mappings.

### Different delimiters for different levels

Use different separators for different levels:

```
root ::= character ("|" character)*
character ::= [A-Z][a-z]+ ":" stats ":" equipment
stats ::= stats-range "," stats-range "," stats-range
stats-range ::= "low" | "medium" | "high" 
equipment ::= weapon-type "," armor-type
weapon-type ::= "Sword" | "Axe" | "Staff" | "Dagger"
armor-type ::= "Leather" | "Robes" | "Chain" | "Plate"
```

This generates: `Gandalf:high,low,low:Staff,Robes|Aragorn:low,high,medium:Sword,Plate` which in JSON would be:

```json
[
  {
    "name": "Gandalf",
    "stats": {
      "hp": "high",
      "mp": "low",
      "level": "low"
    },
    "equipment": {
      "weapon": "Staff",
      "armor": "Robes"
    }
  },
  {
    "name": "Aragorn", 
    "stats": {
      "hp": "low",
      "mp": "high",
      "level": "medium"
    },
    "equipment": {
      "weapon": "Sword",
      "armor": "Plate"
    }
  }
]
```

### Semantic Soundness

One advantage of using JSON is the hints it gives the LLM. 
If it sees `"name": "Gandalf"`, instead of just `Gandalf` it might be more inclined to generate a wizard class or give the character a staff.
The same goes for numbers, the llm does not inherently understand what a good number for a high level or mana pool is - but it understands high vs low.

When designing compact formats:

✅ **Good:** `"Warrior" | "Mage" | "Rogue"`  
✅ **Good:** `"Sword" | "Staff" | "Dagger"`  
✅ **Good:** `"Leather" | "Robes" | "Chain"`  
✅ **Good:** `"Low" | "Medium" | "High"`  

❌ **Bad:** `"WAR" | "MAG" | "ROG"` - abbreviated and potentially ambiguous  
❌ **Bad:** `"W" | "M" | "R"` - arbitrary single letters  
❌ **Bad:** `"1" | "2" | "3"` - numeric values  

The LLM generates text based on semantic understanding. Use full words that align perfectly with how language models think about concepts.  
You should additionally provide the right context and single or few shots prompting to make it more robust.

### Underscores footgun

The GBNF format does not support `_`. According to the [the GBNF format documentation](https://github.com/ggml-org/llama.cpp/tree/master/grammars#json-schemas--gbnf), only lowercase characters and dashes are allowed for naming nonterminals.

## Practical Example: Legendary Weapon Generator

Let's build a weapon generation system that creates legendary weapons for your RPG. We'll start simple and add complexity step by step, showing you how GBNF grammars work in practice.

### Why Use GBNF for Weapon Generation?

Traditional random generators often create nonsensical combinations like "Flaming Sword of Ice", with 8 fire damage and a random generic backstory as well an ice ability. More advanced systems exist but they rely on lookup tables which can become tedious very quickly.
LLMs with GBNF understand semantic coherence - they'll generate "Flamebrand, Ancient Sword of Solar Wrath" instead. 
Which has 8 fire damage, and a meaningful backstory based on how you got it 
or the lore from your game as well as an ability that is chosen based on the backstory, damage and name.

### Step 1: Dynamic Weapon Name Generator

Let's start with a weapon generator that builds weapon names:

**Grammar:**
```
root ::= weapon-name " (" weapon-type ")"
weapon-name ::= name-prefix name-suffix
name-prefix ::= "Flame" | "Frost" | "Shadow" | "Storm" | "Light" | "Dark"
name-suffix ::= "brand" | "fang" | "bane" | "call" | "ward" | "rend"
weapon-type ::= "Sword" | "Axe" | "Dagger" | "Staff" | "Bow" | "Hammer"
```

```gdscript
extends Node

@onready var model = $Model # Your NobodyWhoModel node
@onready var chat = $Chat   # Your NobodyWhoChat node

func _ready():
    # Configure the weapon generator
    model.model_path = "res://models/your-model.gguf"
    chat.model_node = model
    chat.system_prompt = "You are a legendary weapon generator for a fantasy RPG."
    
    # Start the worker so it's ready
    chat.start_worker()
    
    # Connect to handle responses
    chat.response_finished.connect(_on_weapon_generated)

func _input(event):
    if event is InputEventKey and event.pressed and event.keycode == KEY_SPACE:
        generate_weapon()

func generate_weapon():
    chat.set_sampler_preset_grammar(grammar_string)

    # Reset context to avoid new weapons to be influenced by already generated ones.
    chat.reset_context()
    chat.ask("Generate a weapon:")

func _on_weapon_generated(weapon_name: String):
    print(weapon_name)
    # Here you could add the weapon to inventory, display it in UI, etc.
```

**Output examples:**

- `Flamebrand (Sword)`
- `Shadowfang (Dagger)`
- `Stormcall (Staff)`
- `Darkward (Bow)`

This is more or less just a random number generator, but more GPU expensive...

### Step 2: Adding Weapon Stats

Let's add damage and abilities to make weapons more interesting for gameplay, this is where we deviate from a random weapon generator to a semantic weapon generator:

**Grammar:**
```
root ::= weapon-name " (" weapon-type ") - " damage-level " damage, " ability-name " ability. "  backstory
weapon-name ::= name-prefix name-suffix
name-prefix ::= "Flame" | "Frost" | "Shadow" | "Storm" | "Light" | "Dark"
name-suffix ::= "brand" | "fang" | "bane" | "call" | "ward" | "rend"
weapon-type ::= "Sword" | "Axe" | "Dagger" | "Staff" | "Bow" | "Hammer"
damage-level ::= "Low" | "Medium" | "High" | "Legendary"
ability-name ::= "Flame Strike" | "Frost Bite" | "Shadow Step" | "Lightning Bolt" | "Healing Aura" | "Poison Cloud"
backstory ::= [a-zA-Z0-9 ]+ "."
```

Be careful not to add too many symbols in your backstory. If the model can not write a `.` it will increase the chance that it will end the sentence instead of writing paragraph upon paragraph of text.

```gdscript
func generate_weapon():
    chat.set_sampler_preset_grammar(grammar_string)

    # Reset context to avoid new weapons to be influenced by already generated ones.
    chat.reset_context()
    chat.ask("Generate a weapon:")

func _on_weapon_generated(weapon_data: String):
    print(weapon_data)
```

**Output examples:**

- `Shadowfang (Sword) - Legendary damage, Shadow Step ability. Shadowfang is a legendary sword that was forged by the ancient shadow realm.`

See how the examples will match flame and brand to a sword, will give it the flame strike ability as well as a thematic backstory. It feels like there is intent behind the creation of this weapon.

### Step 3: Enhanced Backstories

Let's expand the backstory system to allow for richer, more detailed weapon lore:

**Grammar:**
```
root ::= weapon-name " (" weapon-type ") - " damage-level " damage, " ability-name " ability. Story: " backstory
weapon-name ::= name-prefix name-suffix
name-prefix ::= "Flame" | "Frost" | "Shadow" | "Storm" | "Light" | "Dark"
name-suffix ::= "brand" | "fang" | "bane" | "call" | "ward" | "rend"
weapon-type ::= "Sword" | "Axe" | "Dagger" | "Staff" | "Bow" | "Hammer"
damage-level ::= "Low" | "Medium" | "High" | "Legendary"
ability-name ::= "Flame Strike" | "Frost Bite" | "Shadow Step" | "Lightning Bolt" | "Healing Aura" | "Poison Cloud"
backstory ::= [a-zA-Z0-9 ]{50,200} "."
```

When doing this we want to also inject some of our lore. We will borrow from  Lord of the rings here - replace with your own lore.

```gdscript

func _ready():
  # Configure the weapon generator 
  chat.model_node = model
  chat.system_prompt = "Generate a weapon a backstory in the LOTR universe"
    # ... rest of the setup


func generate_weapon():
    chat.set_sampler_preset_grammar(grammar_string)

    # Reset context to avoid new weapons to be influenced by already generated ones.
    chat.reset_context()
    chat.ask("The party just found a new weapon after travelling through the mines of Moria:")

func _on_weapon_generated(weapon_data: String):
    print(weapon_data)
```

**Output examples:**
- `Shadowfang (Sword) - Legendary damage, Shadow Step ability. The sword is made from the dark shards that were once part of the Balrog`
- `Flamebrand (Sword) - High damage, Flame Strike ability. Backstory involves a fallen dwarf lord named Drakon who was corrupted by the Balrogs and used the sword to slay an enemy.`

### Step 4: Compact Format for Performance

For games that generate many weapons or even very complex weapons, you want maximum efficiency. Let's create a compact pipe-separated format:

**Grammar:**
```
root ::= weapon-name "|" weapon-type "|" damage-level "|" ability-name "|" weight "|" throwable "|" damage-type "|" durability "|" rarity "|" enchantment "|" material "|" short-story
weapon-name ::= name-prefix name-suffix
name-prefix ::= "Flame" | "Frost" | "Shadow" | "Storm" | "Light" | "Dark"
name-suffix ::= "brand" | "fang" | "bane" | "call" | "ward" | "rend"
weapon-type ::= "Sword" | "Axe" | "Dagger" | "Staff" | "Bow" | "Hammer"
damage-level ::= "Low" | "Medium" | "High" | "Legendary"
ability-name ::= "Flame Strike" | "Frost Bite" | "Shadow Step" | "Lightning Bolt" | "Healing Aura" | "Poison Cloud"
weight ::= "Heavy" | "Light"
throwable ::= "Throwable" | "Non-throwable"
damage-type ::= "Sharp" | "Pierce" | "Blunt"
durability ::= "Fragile" | "Sturdy" | "Unbreakable"
rarity ::= "Common" | "Rare" | "Epic" | "Legendary"
enchantment ::= "Glowing" | "Humming" | "Pulsing" | "Silent"
material ::= "Steel" | "Mithril" | "Obsidian" | "Crystal"
backstory ::= [a-zA-Z0-9 ]{50,200} "."
```

**Note:** So-called "thinking" or "reasoning" models will strongly prefer to start every generation with a block of text inside `<think>` tags. If your grammar doesn't naturally allow the output to be prefixed with a "thinking" section like this, it will try to squeeze it into free-text sections (e.g. like the backstory section in the example above). If relying a lot on structured generation, you may prefer to use a "non-thinking" model. If you prefer to keep the "thinking" ability, you could begin your grammar with a section like `"<think>" [a-zA-Z0-9 ]{10,1000} "." "</think>` to allow it to get it's reasoning section out of the way.
Furthermore, the current implementation of GBNF has some performance issues with using specifc ranges (eg: word{10,20}) - so it might be smarter to have a non grammarized model generate the short story.

**Output examples:**
- `Flamebrand|Sword|High|Flame Strike|Heavy|Non-throwable|Sharp|Sturdy|Epic|Glowing|Steel|Forged by fire elementals in ancient volcano`

or with thinking models (demonstrating that it will squeeze in the "thinking" section wherever possible:

- `Shadowfang|Axe|Legendary|Shadow Step|Light|Throwable|Sharp|Sturdy|Epic|Silent|Steel|The Shadowfang is a legendary axe that is said to have been forged in the depths of the Shadowspire Mountains by the elusive Night Hunter.`
- `Stormcall|Staff|Legendary|Lightning Bolt|Light|Non-throwable|Blunt|Unbreakable|Legendary|Pulsing|Crystal|The user wants me to generate a short story for the weapon. I will think...`

---

This is quite a powerful system for procedural generation of anything being weapons, levels, questlines or whatever you can think of, and even better 
You get to influence the generation meaningfully with the prompt that you send, while keeping the variety offered by the system.

This complete system generates weapons with all the attributes your game systems might need, from combat mechanics (damage type, weight) to visual effects (enchantment, material) and lore (story).
