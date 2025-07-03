# Tool Calling
_Triggering actions from within the model._

---

Welcome to the tool calling page!

Now that you have some of the basics understood (if not, please read [Simple Chat](simple-chat.md)), 
we can move on to adding one of the truly powerful and fun components to our model; Tool/Function Calling.

Tool calling is a way to give you model actions to perform in your gameworld.  

The model can:

* Check data - "What's my health?"
* Change the world - "Open the north gate."
* Run helper logic - damage rolls, crafting math, random loot.

this can even be used in combination with GOAP 

We'll start with a small and simple tool, add arguments, then increase accuracy using schema and adding constraints.

**Note that not all models support toolcalling**

---

## A simple tool

This is a examples of how to give the model acces to a function we have created, that gets the players current stats (health, mana, gold).

=== ":simple-godotengine: Godot"
    
    ```gdscript
    extends NobodyWhoChat
    
    func get_player_stats() -> String:
        var player = GameManager.get_local_player()
        return JSON.stringify({
            "health": player.health,
            "mana":   player.mana,
            "gold":   player.gold
        })
    
    func _ready():
        add_tool(get_player_stats, "Returns the local player's health, mana, and gold.")
    ```

=== ":simple-unity: Unity"

    ```csharp
    private int GetPlayerHealth()
    {
        return GameManager.LocalPlayer.health;
    }

    public void Start() 
    {
        chat.AddTool((System.Func<int>)GetPlayerHealth,
                     "Returns the local player's current health");
    }
    ```

    you need to use delegates for the schema-generation to work: 
    In my opionated opnion the prettiest way to do it by casting: like this  `(System.Func<int>)GetPlayerHealth`.  
    System.Func makes it a delegate type, the  <int> defines that the output from this funciton is an int.
    

Ask "How hurt am I?" - the model calls your tool and answers with real numbers.

---


## But i need arguments, you say:

Sure - that is possible, but only primitives are currently implemented in NobodyWho:
Allowed primitive types: `int`, `float`, `bool`, `String`/`string`, `Array`/`string[]`.

This is due to how tool calling is implemented:  
Models operate with json as an abstract layer instead of using a specific language (like godot) when calling tools. 
When NobodyWho recieves a function or a delegate it will deconstruct the name and parameters and use them 
to construct a json schema, that we can pass to the model.

in the example below the generated json will look something like this:

```json
{
  "type": "object",
  "properties": {
    "amount": {
      "type": "integer",
      "description": ""
    }
  },
  "required": ["amount"]
}
```

This is then used to construct a lazy loadable gbnf grammar, so the models always passes the correct amount and set of arguemnts.
A limititation of this is that we cannot extract the description from a given argument. Therefore it might be advantaqgeous to write your own scehma for maximum precision.

=== ":simple-godotengine: Godot"

    ```gdscript
    func heal_player(amount: int) -> String:
        GameManager.get_local_player().heal(amount)
        return "Healed %d HP" % amount

    add_tool(heal_player, "Heals the local player by a number of hit-points")
    ```
    *Godot auto-builds the JSON schema from the type hints.*

=== ":simple-unity: Unity"

    ```csharp
    private string HealPlayer(int amount)
    {
        GameManager.LocalPlayer.Heal(amount);
        return $"Healed {amount} HP";
    }
    
    chat.AddTool((System.Func<int, string>)HealPlayer,
                 "Heals the local player by a number of hit-points");
    ```

    note the System.Func<int, string>, the int is the output and the string is the first parameter.  
    so to create the correct delegate you follow this scheme: System.Func<`output type`, `first param` , `second param`, etc..> - easy!

---


## Acting on the World
Have the model open a door.

=== ":simple-godotengine: Godot"

    ```gdscript
    func open_door(door_id: String) -> String:
        DoorManager.open(door_id)
        return "Opened door %s" % door_id

    add_tool(open_door, "Opens a door in the world by id")
    ```

=== ":simple-unity: Unity"

    ```csharp
    private string OpenDoor(string doorId)
    {
        DoorManager.Open(doorId);
        return $"Opened door {doorId}";
    }
    
    chat.AddTool((System.Func<string, string>)OpenDoor,
                 "Opens a door in the world by id");
    ```

---

## Multiple Tools & Resetting
=== ":simple-godotengine: Godot"

    ```gdscript
    add_tool(get_player_stats, "Player stats")
    add_tool(open_door,        "Open a door")
    ```

=== ":simple-unity: Unity"

    ```csharp
    chat.AddTool((System.Func<int>)GetPlayerHealth, "Player health");
    chat.AddTool((System.Func<string, string>)OpenDoor, "Open a door");
    chat.ClearTools(); // this clears the tools, but also resets the context
    ```

---

## But I don't want it to halloucinate random strings

Dont worry, we got you. 
As i mentioned before, we are using the openschema specification 

```jsonschema
{
  "type": "object",
  "properties": {
    "color": {
      "type": "string",
      "description": "Button colour to press",
      "enum": ["red", "blue", "green"]
    }
  },
  "required": ["color"],
}
```


=== ":simple-godotengine: Godot"

    ```gdscript
    # `press_button_schema` holds the JSON shown above.
    func press_button(color: String) -> String:
        ButtonManager.press(color)
        return "Pressed %s button" % color

    add_tool_with_schema(press_button,
                         press_button_schema,
                         "Press one of the three coloured buttons (red, blue, green)")
    ```

=== ":simple-unity: Unity"

    I lied...  
    We do not in fact 'got you', at least not yet - this feature is still in the works for Unity.

Result: the model **cannot** request any colour other than *red*, *blue*, or *green*.  Use the same pattern for item rarities, quest tiers etc...

**Heads-up** – NobodyWho turns that schema into a GBNF grammar using the open-source [`richardanaya/gbnf`](https://github.com/richardanaya/gbnf) converter.  It currently supports the common bits: primitive types, `enum`, `required`, flat `oneOf`, and simple arrays.  Exotic keywords (`minimum`, `pattern`, deeply-nested refs) may be ignored until the library grows.

---

A note on  descriptions:

The description helps the model pick the right tool and pass the right arguments. Be explicit. Explain when the use the tool, explain what the tool does.
Bad: **"Door"**  
Good: **"Use this function when the assistant is blocked or needs to close a door. This tool opens or closes the door with the given id, if -1 is given, the nearest door will be interacted with."**
