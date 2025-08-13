extends NobodyWhoChat

func run_test():
	# configure node
	model_node = get_node("../ChatModel")
	system_prompt = "You are a helpful assistant, capable of answering questions about the world."

	
	assert(await test_say())
	assert(await test_antiprompts())
	assert(await test_antiprompts_multitokens())
	assert(await test_chat_history())
	assert(await test_stop_generation())
	assert(await test_tool_call())
	return true

func test_say():
	say("Please tell me what the capital city of Denmark is.")

	var response = await response_finished

	print("✨ Got response: " + response)
	assert("Copenhagen" in response)
	return true

func test_antiprompts():
	stop_words = PackedStringArray(["fly"])
	reset_context() # restart the worker to include the antiprompts
	
	say("List these animals in alphabetical order: cat, dog, fly, lion, mouse")
	var response = await response_finished

	print("✨ Got antiprompt response: " + response)

	assert("dog" in response, "Should not stop before the antiprompt")
	assert("fly" in response, "Should reach the antiprompt")
	assert(not "lion" in response, "Should stop at antiprompt")
	assert(not "mouse" in response, "Should not continue past antiprompt")
	
	return true


func test_antiprompts_multitokens():
	stop_words = PackedStringArray(["horse-rider"])
	system_prompt = "You only list the words in alphabetical order. nothing else."

	reset_context() # restart the worker to include the antiprompts
	
	say("List all the words in alphabetical order: dog, horse-rider, lion, mouse")
	var response = await response_finished

	print("✨ Got antiprompt response: " + response)

	assert("dog" in response, "Should not stop before the antiprompt")
	assert("horse-rider" in response, "Should reach the antiprompt")
	assert(not "lion" in response, "Should stop at antiprompt")
	assert(not "mouse" in response, "Should not continue past antiprompt")
	
	return true

func test_chat_history():
	# Reset to clean state
	stop_words = PackedStringArray()
	reset_context()
	
	# Set up a simple chat history
	var messages = [
		{"role": "user", "content": "What is 2 + 2?"},
		{"role": "assistant", "content": "2 + 2 equals 4."}
	]
	
	set_chat_history(messages)
	var retrieved_messages = await get_chat_history()
	print("✨ Retrieved chat history: " + str(retrieved_messages))
	
	# Basic validation
	assert(retrieved_messages.size() == 2, "Should have 2 messages")
	assert(retrieved_messages[0]["role"] == "user", "First message should be from user")
	assert("2 + 2" in retrieved_messages[0]["content"], "First message should contain the question")
	assert(retrieved_messages[1]["role"] == "assistant", "Second message should be from assistant")
	assert("4" in retrieved_messages[1]["content"], "Second message should contain the answer")

	say("What did I just ask you about?")
	var resp = await response_finished
	print("Got resp: " + resp)
	assert("2 + 2" in resp)
	return true
	

func current_temperature(location: String, zipCode: int, inDenmark: bool) -> String:
	push_warning("current_temperature: %s, %d, %s" % [location, zipCode, inDenmark])
	if location.to_lower() == "copenhagen":
		return "12.34"
	return "Unknown city name"


func test_tool_call():
	self.add_tool(current_temperature, "Gets the current temperature in city.")
	self.system_prompt = "You're a helpful tool-calling assistant. Remember to keep proper tool calling syntax."
	self.reset_context()
	say("I'd like to know the current temperature in Copenhagen. with zipcode 12.3 and in denmark is true")
	var response = await response_finished
	print(response)
	assert("12.34" in response)
	return true

func test_stop_generation():
	print("✨ Testing stop generation")
	system_prompt = "You're countbot. A robot that's very good at counting"
	reset_context()

	# XXX: this signal is never disconnected
	self.response_updated.connect(func(token: String):
		if "2" in token:
			stop_generation()
	)
	say("count from 0 to 9")
	
	var response = await response_finished

	print("✨ Got response: " + response)
	assert("2" in response, "Should stop at 2")
	assert(not "8" in response, "Should not continue past 2")

	# test get/set history w/ tool call messages in there
	var messages = await get_chat_history()
	set_chat_history(messages)
	var messages_again = await get_chat_history()
	assert(messages == messages_again)

	# clean up: disconnect signal handler
	for dict in response_updated.get_connections():
		response_updated.disconnect(dict.callable)

	return true
