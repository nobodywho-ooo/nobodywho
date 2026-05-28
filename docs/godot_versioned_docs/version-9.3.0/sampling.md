# Sampling
_Controlling how the model picks tokens and constraining output format._

---

The model does not produce tokens directly but rather a probability distribution over all possible tokens. We must then choose how to pick the next token from the distribution. This is the job of a **sampler**, which using NobodyWho you can freely modify to achieve better quality outputs or constrain the outputs to some known format (e.g. JSON).

## Sampler Presets

NobodyWho offers several built-in presets you can apply to your `NobodyWhoChat` node:

### JSON Output

Force the model to always produce valid JSON:

```gdscript
chat.set_sampler_preset_json()
chat.system_prompt = "Generate a character with name, weapon, and armor properties."
chat.ask("Create a fantasy character")
# Output will always be valid JSON, e.g.:
# {"name": "Eldara", "weapon": "enchanted bow", "armor": "leather vest"}
```

### Temperature

Control the "creativity" of the model. Lower values make the model more deterministic:

```gdscript
chat.set_sampler_preset_temperature(0.2)  # More focused/deterministic
chat.set_sampler_preset_temperature(1.5)  # More creative/random
```

### Greedy

Always pick the most probable token:

```gdscript
chat.set_sampler_preset_greedy()
```

## Structured Output

One of the most powerful features is constraining the model to produce output in a specific format. This gives you a hard guarantee that the output matches your format, rather than relying on the model to get it right on its own.

### Grammar Constraints

You can constrain the model's output using a GBNF grammar:

```gdscript
var grammar = """
root ::= greeting " " name
greeting ::= "Hello" | "Hi" | "Hey"
name ::= "World" | "Friend" | "There"
"""
chat.set_sampler_preset_constrain_with_grammar(grammar)
```

This makes it **impossible** for the model to generate anything outside your defined format.

For a comprehensive tutorial on writing GBNF grammars, including JSON generation, compact formats, and practical game examples, see the [Structured Output](structured-output.md) guide.

### JSON Schema Constraints

Force the model to produce JSON matching a specific schema:

```gdscript
var schema = JSON.stringify({
    "type": "object",
    "properties": {
        "name": {"type": "string"},
        "level": {"type": "integer"},
        "class": {"type": "string", "enum": ["Warrior", "Mage", "Rogue"]}
    },
    "required": ["name", "level", "class"]
})
chat.set_sampler_preset_constrain_with_json_schema(schema)
```

### Regex Constraints

For simpler patterns, constrain the output with a regular expression:

```gdscript
# Force the model to answer with exactly "yes" or "no"
chat.set_sampler_preset_constrain_with_regex("yes|no")
```

## Changing Samplers Mid-Conversation

You can change the sampler at any point during a conversation. The new sampler will take effect on the next `ask()` call:

```gdscript
# Start with free-form chat
chat.ask("Tell me about yourself")
var response = await chat.response_finished

# Switch to structured output for the next question
chat.set_sampler_preset_json()
chat.ask("Now describe your stats as JSON")
var json_response = await chat.response_finished
```
