extends NobodyWhoChat

func run_test():
	# configure node
	model_node = get_node("../ChatModel")
	system_prompt = "You are a helpful assistant, capable of answering questions about the world."

	
	assert(await test_say())
	assert(await test_antiprompts())
	assert(await test_antiprompts_multitokens())
	assert(await test_stop_generation())
	assert(await test_chat_history())
	return true

func test_say():
	say("Please tell me what the capital city of Denmark is.")

	var response = await response_finished

	print("✨ Got response: " + response)
	assert("Copenhagen" in response)
	return true

func test_antiprompts():
	stop_words = PackedStringArray(["fly"])
	start_worker() # restart the worker to include the antiprompts
	
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

	start_worker() # restart the worker to include the antiprompts
	
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
	start_worker()
	
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
	assert("2 + 2" in resp)
	
	return true


func test_stop_generation():
	print("✨ Testing stop generation")
	start_worker()

	self.response_updated.connect(func(token: String):
		if token == "5":
			stop_generation()
	)
	say("count from 0 to 9")
	
	var response = await response_finished

	print("✨ Got response: " + response)
	assert("5" in response, "Should stop at 5")
	assert(not "6" in response, "Should not continue past 5")
	
	return true