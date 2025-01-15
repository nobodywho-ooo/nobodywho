extends NobodyWhoChat

func run_test():
	# configure node
	model_node = get_node("../ChatModel")
	system_prompt = "You are a helpful assistant, capable of answering questions about the world."

	# say soemthing
	say("Please tell me what the capital city of Denmark is.")

	# wait for the response
	var response = await response_finished
	print("✨ Got response: " + response)
	assert("Copenhagen" in response)
	return true
