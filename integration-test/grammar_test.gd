extends Node

@onready var model = $Model
@onready var chat = $Chat

func _ready():
	
	# Configure the chat
	chat.model_node = model
    # purposefully not mentioning the grammar in the system prompt
	chat.system_prompt = "You are a character creator for a fantasy game. You will be given a list of properties and you will need to fill out thoe properties.s"
	
	# Start worker first to ensure model is loaded
	chat.start_worker()
	
	print("Model loaded, configuring sampler...")
	
	# Configure the sampler with JSON grammar
	var sampler = NobodyWhoSampler.new()
	sampler.method = "Temperature" # Set method first
	sampler.temperature = 0.8 # Set temperature
	sampler.seed = randi() % 1000 # Now seed will work (and value is within u32 range)
	
	print("Setting up grammar...")
	sampler.use_grammar = true
	sampler.grammar_path = "res://grammars/json.gbnf"
	chat.sampler = sampler
	
	test_json_output()

func test_json_output():
	print("Testing JSON grammar...")
	
	# Request a JSON response with explicit structure
	chat.say("""Generate exactly these properties:
	- name (string)
	- class (string)
	- level (number)
	""")
	
	var response = await chat.response_finished
	print("\nResponse received: \n", response)
	
	# Validate it's proper JSON
	var json = JSON.new()
	var error = json.parse(response)
	if error == OK:
		print("\nSuccess! Valid JSON received! 🎉")
		print("Parsed data:", json.data)
	else:
		print("\nError! Invalid JSON received 😢")
		print("Parse error at line ", json.get_error_line(), ": ", json.get_error_message())
	
	assert(json.data.has("name"))
	assert(json.data.has("class"))
	assert(json.data.has("level"))
