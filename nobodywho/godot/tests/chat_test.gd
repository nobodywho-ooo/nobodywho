extends Node
# Phase 2 tests for NobodyWhoChat query/mutation methods. These need a live
# chat, which needs a model, so the suite self-skips when TEST_MODEL is unset.
#
# Run: TEST_MODEL=/path/to/model.gguf \
#      nix shell nixpkgs#godot_4 --command godot --headless --path .
#
# Every async #[func] (create() included) returns an awaitable Variant
# directly (the internal task's value-or-Signal `wait()` result), so you
# `await chat.foo()`. Await the return value immediately — storing it and
# awaiting after another await/frame is unsupported.

var _model_path: String

func run(runner: Node) -> void:
	_model_path = OS.get_environment("TEST_MODEL")
	if _model_path.is_empty():
		print("SKIP: chat_test needs TEST_MODEL env var (point it at a .gguf)")
		return

	var chat = await _make_chat(runner)
	if chat == null:
		runner.fail("chat_test: could not create chat (check TEST_MODEL path)")
		return

	await _test_system_prompt(runner, chat)
	await _test_template_variables(runner, chat)
	await _test_sampler_config(runner, chat)
	await _test_stats(runner, chat)
	await _test_tokenize(runner, chat)
	await _test_chat_history(runner, chat)

	# RefCounted: chat is refcount-managed, nothing to free.

func _make_chat(runner: Node):
	return await NobodyWhoChat.create(_model_path, {})

func _test_system_prompt(runner: Node, chat) -> void:
	# Initially no system prompt (we created with {}).
	var got = await chat.get_system_prompt()
	if got == null:
		runner.ok("system_prompt: initially null")
	else:
		runner.fail("system_prompt: expected null initially, got %s" % str(got))

	# Set a prompt, read it back.
	await chat.set_system_prompt("You are a test assistant.")
	got = await chat.get_system_prompt()
	if got == "You are a test assistant.":
		runner.ok("system_prompt: set/get round-trip")
	else:
		runner.fail("system_prompt: expected 'You are a test assistant.', got %s" % str(got))

	# Clear it.
	await chat.set_system_prompt(null)
	got = await chat.get_system_prompt()
	if got == null:
		runner.ok("system_prompt: clear via null")
	else:
		runner.fail("system_prompt: expected null after clear, got %s" % str(got))

func _test_template_variables(runner: Node, chat) -> void:
	await chat.set_template_variable("enable_thinking", false)
	var vars = await chat.get_template_variables()
	if vars is Dictionary and vars.get("enable_thinking", null) == false:
		runner.ok("template_variables: set/get single")
	else:
		runner.fail("template_variables: enable_thinking not false, got %s" % str(vars))

	# Bulk replace.
	var bulk := {"enable_thinking": true, "custom_flag": false}
	await chat.set_template_variables(bulk)
	vars = await chat.get_template_variables()
	if vars is Dictionary and vars.get("enable_thinking", null) == true and vars.get("custom_flag", null) == false:
		runner.ok("template_variables: bulk set/get")
	else:
		runner.fail("template_variables: bulk result wrong, got %s" % str(vars))

func _test_sampler_config(runner: Node, chat) -> void:
	var preset := NobodyWhoSamplerPresets.temperature(0.123)
	await chat.set_sampler_config(preset)
	var got = await chat.get_sampler_config()
	if got == null:
		runner.fail("sampler_config: get returned null after set")
		return
	var got_json: String = got.to_json()
	if got_json.find("0.123") >= 0:
		runner.ok("sampler_config: set/get round-trip (temperature preserved)")
	else:
		runner.fail("sampler_config: temperature 0.123 not found in %s" % got_json)

	# from_json / to_json round-trip.
	var parsed = NobodyWhoSamplerConfig.from_json(got_json)
	if parsed == null:
		runner.fail("sampler_config: from_json returned null")
	else:
		var reparsed_json: String = parsed.to_json()
		if reparsed_json == got_json:
			runner.ok("sampler_config: to_json/from_json round-trip")
		else:
			runner.fail("sampler_config: json round-trip mismatch: %s vs %s" % [got_json, reparsed_json])

	# Builder chain.
	var built := NobodyWhoSamplerBuilder.new().top_k(7).temperature(0.5).greedy()
	await chat.set_sampler_config(built)
	var got2 = await chat.get_sampler_config()
	var bj: String = got2.to_json()
	# Greedy preset has no sample-step field name in JSON; check for top_k=7 and "Greedy".
	# Greedy preset serializes sample_step as lowercase "greedy".
	if bj.find("7") >= 0 and bj.find("greedy") >= 0:
		runner.ok("sampler_config: builder chain (top_k=7, greedy)")
	else:
		runner.fail("sampler_config: builder chain wrong, got %s" % bj)

func _test_stats(runner: Node, chat) -> void:
	var stats = await chat.get_stats()
	if stats is Dictionary:
		var ctx_size = stats.get("context_size", null)
		var ctx_used = stats.get("context_used", null)
		if ctx_size is int and ctx_size > 0 and ctx_used is int and ctx_used >= 0:
			runner.ok("stats: context_size=%d context_used=%d" % [ctx_size, ctx_used])
		else:
			runner.fail("stats: bad shape, got %s" % str(stats))
	else:
		runner.fail("stats: expected Dictionary, got %s" % str(stats))

func _test_tokenize(runner: Node, chat) -> void:
	var ids = await chat.tokenize("hello")
	if ids is Array and ids.size() > 0:
		var first = ids[0]
		if first is int and first >= 0:
			runner.ok("tokenize: 'hello' -> %d token(s), first=%d" % [ids.size(), first])
		else:
			runner.fail("tokenize: first id not a non-negative int, got %s" % str(first))
	else:
		runner.fail("tokenize: expected non-empty Array, got %s" % str(ids))

func _test_chat_history(runner: Node, chat) -> void:
	# Start from a clean slate.
	await chat.reset_history()
	var hist = await chat.get_chat_history()
	if hist is Array and hist.is_empty():
		runner.ok("chat_history: empty after reset")
	else:
		runner.fail("chat_history: expected empty Array after reset, got %s" % str(hist))
		return

	# Set a simple list-of-dicts (the common case) and read it back.
	var msgs := [
		{"role": "user", "content": "Hi there"},
		{"role": "assistant", "content": "Hello!"},
	]
	await chat.set_chat_history(msgs)
	hist = await chat.get_chat_history()
	if hist is Array and hist.size() == 2:
		var r0 = hist[0].get("role", "")
		var c0 = hist[0].get("content", "")
		var r1 = hist[1].get("role", "")
		var c1 = hist[1].get("content", "")
		if r0 == "user" and c0 == "Hi there" and r1 == "assistant" and c1 == "Hello!":
			runner.ok("chat_history: set/get list-of-dicts round-trip")
		else:
			runner.fail("chat_history: round-trip values wrong: [%s/%s, %s/%s]" % [r0, c0, r1, c1])
	else:
		runner.fail("chat_history: expected 2-element Array, got %s" % str(hist))
