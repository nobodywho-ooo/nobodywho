extends Node

@onready var model = $Model
@onready var chat = $Chat

func _ready():
	
	# Configure the chat
	chat.model_node = model
    # purposefully not mentioning the grammar in the system prompt
	chat.system_prompt = "You are a character creator for a fantasy game. You will be given a list of properties and you will need to fill out thoe properties.s"
	
	# Start worker first to ensure model is loaded
	
	print("Model loaded, configuring sampler...")
	

	var sampler = NobodyWhoSampler.new()
	sampler.method = "Temperature"
	sampler.temperature = 0.8

	# Configure the sampler with JSON grammar
	sampler.use_grammar = true
	sampler.grammar_path = "res://grammars/json.gbnf"
	sampler.root_def = "root"
	
	chat.sampler = sampler
	chat.start_worker()
	chat.response_updated.connect(func(res):
		print(res)
	)
	
	
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
