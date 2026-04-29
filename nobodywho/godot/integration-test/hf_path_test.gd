extends Node

# Verifies that a `huggingface:` model path resolves through the core download cache.
# Runs offline inside the nix sandbox: the build symlinks the pre-fetched model into
# $XDG_CACHE_HOME/nobodywho/models/NobodyWho/Qwen_Qwen3-0.6B-GGUF/ so core finds it
# without ever touching the network.

const HF_MODEL_PATH := "huggingface:NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf"


func run_test() -> bool:
	print("🌟 Starting hf_path_test")

	var model := NobodyWhoModel.new()
	model.model_path = HF_MODEL_PATH
	add_child(model)

	var chat := NobodyWhoChat.new()
	chat.model_node = model
	chat.allow_thinking = false
	chat.system_prompt = "You are a helpful assistant. Answer concisely."
	add_child(chat)

	# Surface a load failure loudly — otherwise the test would hang.
	chat.worker_failed.connect(func(err: String):
		push_error("❌ hf_path_test worker_failed: " + err)
		get_tree().quit(1)
	)

	chat.start_worker()
	await chat.worker_started
	print("✨ hf_path_test worker_started fired (model resolved via cache)")

	chat.ask("Please tell me what the capital city of Denmark is.")
	var response: String = await chat.response_finished
	print("✨ hf_path_test got response: " + response)
	assert("Copenhagen" in response, "Expected 'Copenhagen' in the response")

	return true
