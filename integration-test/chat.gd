extends NobodyWhoChat

func run_test():
	# configure node
	model_node = get_node("../ChatModel")
	system_prompt = "You are a helpful assistant, capable of answering questions about the world."

	
	assert(await test_say())
	assert(await test_antiprompts())

	return true

func test_say():
	say("Please tell me what the capital city of Denmark is.")

	var response = await response_finished

	print("✨ Got response: " + response)
	assert("Copenhagen" in response)
	return true

func test_antiprompts():
	stop_tokens = PackedStringArray(["horse"])
	start_worker() # restart the worker to include the antiprompts
	
	say("List these animals in alphabetical order: cat, dog, giraffe, horse, lion, mouse")
	var response = await response_finished

	print("✨ Got antiprompt response: " + response)

	assert("giraffe" in response, "Should not stop before the antiprompt")
	assert("horse" in response, "Should reach the antiprompt")
	assert(not "lion" in response, "Should stop at antiprompt")
	assert(not "mouse" in response, "Should not continue past antiprompt")
	
	return true
