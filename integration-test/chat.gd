extends NobodyWhoChat

func run_test():
	# configure node
	model_node = get_node("../ChatModel")
	system_prompt = "You are a helpful assistant, capable of answering questions about the world."

	test_say()
	test_antiprompts()

	return true

func test_say():
	say("Please tell me what the capital city of Denmark is.")

	var response = await response_finished

	print("✨ Got response: " + response)
	assert("Copenhagen" in response)
	return true

func test_antiprompts():
	sampler.antiprompts = ["lion"]
	start_worker() # restart the worker to include the antiprompts
    
	say("List these animals in order: horse, giraffe, lion, dog, cat, mouse")
    var response = await response_finished
    print("✨ Got antiprompt response: " + response)

    assert("giraffe" in response, "Should not stop before the antiprompt")    
    assert("lion" in response, "Should reach the antiprompt")
    assert(not "dog" in response, "Should stop at antiprompt")
    assert(not "cat" in response, "Should not continue past antiprompt")
    
    return true
