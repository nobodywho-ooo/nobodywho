class_name test_grammar extends Node

@onready var model = $Model
@onready var chat = $Chat

func _ready():    
	chat.model_node = model
	# purposefully not mentioning the grammar in the system prompt
	chat.system_prompt = "You are a character creator for a fantasy game. You will be given a list of properties and you will need to fill out thoe properties.s"

	chat.sampler = NobodyWhoSampler.new()
	chat.sampler.method = "Greedy"
	chat.sampler.use_grammar = true
		
	chat.start_worker()
 
func run_test() -> bool:
	var result = await test_json_output()
	var result2 = await test_custom_grammar()
	return true

func test_custom_grammar():

	chat.sampler.gbnf_grammar = "root ::= \"Should not use this\" \nname ::= \"John The Flipper Ripper\""
	chat.sampler.root_def = "name"
	chat.start_worker()

	chat.say("Generate a common name")
	var response = await chat.response_finished
	print("✨ Got response: " + response)
	assert(response == "John The Flipper Ripper")

func test_json_output():

	# purposefully not mentioning the grammar type in the system prompt
	chat.say("""Generate exactly these properties:
	- name
	- class
	- level
	""")
	
	var response = await chat.response_finished
	print("✨ Got response: " + response)
	var json = JSON.new()
	var error = json.parse(response)
	if error == OK:
		print("\nValid JSON received")
	else:
		print("\nError! Invalid JSON received")
		print("Parse error at line ", json.get_error_line(), ": ", json.get_error_message())
	
	assert(json.data.has("name"))
	assert(json.data.has("class"))
	assert(json.data.has("level"))
