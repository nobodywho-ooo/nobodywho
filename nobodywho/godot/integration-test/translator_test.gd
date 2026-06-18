extends Node


func run_test() -> bool:
	var model_path := OS.get_environment("TEST_TRANSLATE_MODEL")
	if model_path.is_empty():
		print("⏭️ translator_test: TEST_TRANSLATE_MODEL not set, skipping")
		return true

	var model := NobodyWhoModel.new()
	model.model_path = model_path
	add_child(model)

	var translator := NobodyWhoTranslator.new()
	translator.model_node = model
	translator.source_lang_code = "en"
	translator.target_lang_code = "da"
	add_child(translator)

	translator.worker_failed.connect(func(err: String):
		push_error("❌ translator_test worker_failed: " + err)
		get_tree().quit(1)
	)

	translator.start_worker()
	await translator.worker_started

	translator.translate("Hello, how are you?")
	var response: String = await translator.response_finished
	print("✨ translator_test got response: " + response)
	assert("Hej" in response, "Expected 'Hej' in translation result")

	return true
