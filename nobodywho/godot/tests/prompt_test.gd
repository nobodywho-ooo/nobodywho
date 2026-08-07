extends Node
# Tests for NobodyWhoPrompt and the String|NobodyWhoPrompt dispatch on
# NobodyWhoChat.ask / .tokenize.
#
# Tier 1 (model-less) always runs — it checks the part factories, create,
# from_json, and the bad-element / bad-type error paths.
# Tier 2 (vision) needs a multimodal GGUF + an image file with known content,
# and self-skips without TEST_VISION_MODEL + TEST_IMAGE_FILE.
#
# Run tier 1 alone:
#   nix shell nixpkgs#godot_4 --command godot --headless --path .
# Run tier 2:
#   TEST_VISION_MODEL=/path/to/multimodal.gguf \
#   TEST_IMAGE_FILE=/path/to/known_image.png \
#   nix shell nixpkgs#godot_4 --command godot --headless --path .

func run(runner: Node) -> void:
	await _test_part_factories(runner)
	await _test_create_empty(runner)
	await _test_create_mixed(runner)
	await _test_create_bad_elements(runner)
	await _test_from_json_ok(runner)
	await _test_from_json_error(runner)
	await _test_vision(runner)

# --- Tier 1: model-less ------------------------------------------------------

func _test_part_factories(runner: Node) -> void:
	var t := NobodyWhoPrompt.text("hello")
	if t is Dictionary and t.get("type") == "text" and t.get("value") == "hello":
		runner.ok("prompt: text() factory shape")
	else:
		runner.fail("prompt: text() = %s" % str(t))

	var i := NobodyWhoPrompt.image("res://x.png")
	if i is Dictionary and i.get("type") == "image" and i.get("value") == "res://x.png":
		runner.ok("prompt: image() factory shape (path stored unresolved)")
	else:
		runner.fail("prompt: image() = %s" % str(i))

	var a := NobodyWhoPrompt.audio("res://clip.wav")
	if a is Dictionary and a.get("type") == "audio" and a.get("value") == "res://clip.wav":
		runner.ok("prompt: audio() factory shape (path stored unresolved)")
	else:
		runner.fail("prompt: audio() = %s" % str(a))

func _test_create_empty(runner: Node) -> void:
	var p = NobodyWhoPrompt.create([])
	if p != null:
		runner.ok("prompt: create([]) returns non-null (degenerate but legal)")
	else:
		runner.fail("prompt: create([]) returned null")

func _test_create_mixed(runner: Node) -> void:
	var p = NobodyWhoPrompt.create([
		NobodyWhoPrompt.text("Describe this:"),
		NobodyWhoPrompt.image("res://photo.jpg"),
		NobodyWhoPrompt.text("And this sound:"),
		NobodyWhoPrompt.audio("res://clip.wav"),
	])
	if p != null:
		runner.ok("prompt: create([text, image, text, audio]) builds")
	else:
		runner.fail("prompt: mixed create returned null")

func _test_create_bad_elements(runner: Node) -> void:
	# A bad element aborts create: null + godot_error! (consistent with
	# from_json). Each case below should return null.
	var cases := [
		["not a dictionary"],                                       # not a Dictionary
		[{"type": "bogus", "value": "x"}],                          # unknown type
		[{"type": "text"}],                                         # missing "value"
		[{"value": "nope"}],                                        # missing "type"
	]
	var all_ok := true
	for c in cases:
		var p = NobodyWhoPrompt.create(c)
		if p != null:
			all_ok = false
			runner.fail("prompt: create(%s) expected null, got %s" % [str(c), str(p)])
	if all_ok:
		runner.ok("prompt: create aborts on a bad element (null + godot_error!)")

func _test_from_json_ok(runner: Node) -> void:
	var p = NobodyWhoPrompt.from_json({"role": "user", "content": "hi"})
	if p != null:
		runner.ok("prompt: from_json(dict) returns non-null")
	else:
		runner.fail("prompt: from_json(dict) returned null")

func _test_from_json_error(runner: Node) -> void:
	# An Object is not JSON-representable -> null + godot_error!.
	var obj := Node.new()
	var p = NobodyWhoPrompt.from_json(obj)
	obj.free()
	if p == null:
		runner.ok("prompt: from_json(non-JSON-representable) returns null")
	else:
		runner.fail("prompt: from_json(non-JSON) returned non-null %s" % str(p))

# --- Tier 2: vision (self-skips) ---------------------------------------------

func _test_vision(runner: Node) -> void:
	var model_path: String = OS.get_environment("TEST_VISION_MODEL")
	var image_path: String = OS.get_environment("TEST_IMAGE_FILE")
	var mmproj_path: String = OS.get_environment("TEST_VISION_MMPROJ")
	if model_path.is_empty() or image_path.is_empty():
		print("SKIP: prompt_test vision tier needs TEST_VISION_MODEL + TEST_IMAGE_FILE")
		return

	# Load the model with its MTMD projector (if given), then build the chat
	# from that model. Without a projector, image tokenization fails inside
	# core — so TEST_VISION_MMPROJ is effectively required for the image
	# assertions below, but we don't hard-skip on it (the bad-type paths still
	# exercise parse_prompt without a projector).
	var chat
	if not mmproj_path.is_empty():
		var model = await NobodyWhoModel.create(model_path, {"mmproj_path": mmproj_path})
		if model == null:
			runner.fail("prompt vision: could not load model (check TEST_VISION_MODEL / TEST_VISION_MMPROJ)")
			return
		chat = await NobodyWhoChat.create(model, {})
	else:
		chat = await NobodyWhoChat.create(model_path, {})
	if chat == null:
		runner.fail("prompt vision: could not create chat (check TEST_VISION_MODEL)")
		return

	# Bad-type dispatch: ask(123) and tokenize(123) return null + godot_error!.
	var bad_ask = chat.ask(123)
	if bad_ask == null:
		runner.ok("prompt vision: ask(non-string/non-prompt) returns null")
	else:
		runner.fail("prompt vision: ask(123) returned non-null %s" % str(bad_ask))

	var bad_tok = await chat.tokenize(123)
	if bad_tok == null:
		runner.ok("prompt vision: tokenize(non-string/non-prompt) returns null")
	else:
		runner.fail("prompt vision: tokenize(123) returned non-null %s" % str(bad_tok))

	# Text-only prompt via NobodyWhoPrompt behaves like a plain string ask.
	var text_prompt = NobodyWhoPrompt.create([NobodyWhoPrompt.text("Say only the word: hello")])
	var stream = chat.ask(text_prompt)
	if stream == null:
		runner.fail("prompt vision: text-only ask(prompt) returned null")
		return
	var text: String = await stream.call("completed")
	if not text.is_empty():
		runner.ok("prompt vision: text-only ask(prompt) generated a response")
	else:
		runner.fail("prompt vision: text-only ask(prompt) produced empty text")

	# Multimodal: image prompt. tokenize should include at least one null slot
	# (the image embedding position).
	var img_prompt = NobodyWhoPrompt.create([
		NobodyWhoPrompt.text("What is in this image?"),
		NobodyWhoPrompt.image(image_path),
	])
	var ids = await chat.tokenize(img_prompt)
	if ids is Array:
		var has_null := false
		for id in ids:
			if id == null:
				has_null = true
				break
		if has_null:
			runner.ok("prompt vision: tokenize(image prompt) has a media (null) slot")
		else:
			runner.fail("prompt vision: tokenize(image prompt) had no null slots: %s" % str(ids))
	else:
		runner.fail("prompt vision: tokenize(image prompt) expected Array, got %s" % str(ids))

	# Full multimodal generation. Loose check: non-empty response.
	var vstream = chat.ask(img_prompt)
	if vstream == null:
		runner.fail("prompt vision: multimodal ask returned null")
		return
	var vtext: String = await vstream.call("completed")
	if not vtext.is_empty():
		runner.ok("prompt vision: multimodal ask generated a response (%d chars)" % vtext.length())
	else:
		runner.fail("prompt vision: multimodal ask produced empty text")
