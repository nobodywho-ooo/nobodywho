class_name test_grammar extends Node

@onready var model = $Model # type: NobodyWhoModel
@onready var chat = $Chat

func run_test() -> bool:
	chat.set_log_level("TRACE")
	print("🌟 Starting grammar test")
	chat.model_node = model
	# purposefully not mentioning the grammar in the system prompt
	chat.system_prompt = "You are a character creator for a fantasy game. You will be given a list of properties and you will need to fill out thoe properties.s"

	# I used this webapp to make a gbnf from a json schema
	# https://adrienbrault.github.io/json-schema-to-gbnf/
	# XXX: needed to :%s/\\/\\\\/g afterwards to escape the backslashes
	var gbnf_grammar = """
root ::= "{" ws01 root-name "," ws01 root-class "," ws01 root-level "}" ws01
root-name ::= "\\"name\\"" ":" ws01 string
root-class ::= "\\"class\\"" ":" ws01 ("\\"fighter\\"" | "\\"ranger\\"" | "\\"wizard\\"")
root-level ::= "\\"level\\"" ":" ws01 "1"


value  ::= (object | array | string | number | boolean | null) ws

object ::=
  "{" ws (
	string ":" ws value
	("," ws string ":" ws value)*
  )? "}"

array  ::=
  "[" ws01 (
			value
	("," ws01 value)*
  )? "]"

string ::=
  "\\"" (string-char)* "\\""

string-char ::= [^"\\\\] | "\\\\" (["\\\\/bfnrt] | "u" [0-9a-fA-F] [0-9a-fA-F] [0-9a-fA-F] [0-9a-fA-F]) # escapes

number ::= integer ("." [0-9]+)? ([eE] [-+]? [0-9]+)?
integer ::= "-"? ([0-9] | [1-9] [0-9]*)
boolean ::= "true" | "false"
null ::= "null"

# Optional space: by convention, applied in this grammar after literal chars when allowed
ws ::= ([ \\t\\n] ws)?
ws01 ::= ([ \\t\\n])?
	"""
	chat.set_sampler_preset_grammar(gbnf_grammar)

	chat.start_worker()

	var result = await test_json_output()
	return true

func test_json_output():

	# purposefully not mentioning the grammar type in the system prompt
	chat.ask("""Generate a json object containing exactly these properties:
	- name (a short string)
	- class (either fighter, ranger, or wizard)
	- level (an integer between 1 and 3)
	""")
	
	var response = await chat.response_finished
	print("✨ Got response: " + response)
	var json = JSON.new()
	var error = json.parse(response)
	if error == OK:
		print("\\nValid JSON received")
	else:
		print("\\nError! Invalid JSON received")
		print("Parse error at line ", json.get_error_line(), ": ", json.get_error_message())
	
	assert(json.data.has("name"))
	assert(json.data.has("class"))
	assert(json.data.has("level"))
