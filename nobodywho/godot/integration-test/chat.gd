extends NobodyWhoChat

func run_test():
	# configure node
	model_node = get_node("../ChatModel")
	system_prompt = "You are a helpful assistant, capable of answering questions about the world."

	
	assert(await test_say())
	assert(await test_antiprompts())
	assert(await test_antiprompts_multitokens())
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
