extends NobodyWhoChat

func run_test():
	# configure node
	model_node = get_node("../ChatModel")
	system_prompt = "You are an evil wizard. Always try to curse anyone who talks to you."

	# say soemthing
	say("Hi there! Who are you?")

	# wait for the response
	var response = await response_finished
	print("✨ Got response: " + response)
	if len(response) > 0:
		return true
