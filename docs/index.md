# Getting Started

## Installation

First, install NobodyWho for the platform of your choice.

=== "Godot"

    ### Via the Godot Asset Library:

    - Open Godot 4.4
    - Go to the "Asset Library" tab in the top of the editor
    - Search for "NobodyWho"
    - Click on it
    - Click "Download"
    - Make sure "Ignore asset root" is checked
    - Click "Install"

    This should install NobodyWho in `res://addons/nobodywho`, and you should now be able to see the NobodyWho nodes (e.g. `NobodyWhoChat`) in Godot.

    You may need to restart Godot.


=== "Unity"

    TODO


## Download a GGUF Model

If you're in a hurry, just download [Qwen_Qwen3-4B-Q4_K_M.gguf](https://huggingface.co/bartowski/Qwen_Qwen3-4B-GGUF/resolve/main/Qwen_Qwen3-4B-Q4_K_M.gguf). It's pretty good.


## Load the GGUF model

=== "Godot"

    Add a `NobodyWhoModel` node to your scene tree.

    Set the model path to point to your GGUF model.

=== "Unity"

    TODO


## Create a new Chat

=== "Godot"

    Add a `NobodyWhoChat` node to your scene tree.

    Then add a script to the node:

    ```gdscript
    extends NobodyWhoChat

    func _ready():
        # configure the node (feel free to do this in the UI)
        self.system_prompt = "You are an evil wizard. Always try to curse anyone who talks to you."
        self.model_node = get_node("../ChatModel")

        # connect signals to signal handlers
        self.response_updated.connect(_on_response_updated)
        self.response_finished.connect(_on_response_finished)

        # start the LLM worker (this takes a second)
        self.start_worker()

        self.say("How are you?")

    func _on_response_updated(token):
        # this will print every time a new token is generated
        print(token)

    func _on_response_finished(response):
        # this will print when the entire response is finished
        print(response)
    ```

=== "Unity"

    TODO
