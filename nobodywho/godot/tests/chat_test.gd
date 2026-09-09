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
	await _test_sampler_constraints(runner, chat)
	await _test_stats(runner, chat)
	await _test_tokenize(runner, chat)
	await _test_chat_history(runner, chat)
	await _test_complete(runner, chat)

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

func _test_sampler_constraints(runner: Node, chat) -> void:
	# Constrain output with a regex / JSON schema on the builder.
	await chat.set_template_variable("enable_thinking", false)
	await chat.set_system_prompt("You are a helpful assistant, capable of answering questions about the world.")

	# top_k(1) first on purpose: the constraint must run before it (core
	# prepends constraining steps), or the single surviving candidate is
	# unlikely to be grammar-valid and generation aborts. The literal is one
	# no model would answer with on its own, so an exact match proves the
	# constraint applied.
	var regex_cfg := NobodyWhoSamplerBuilder.new().top_k(1).regex("zqxjvkw").dist()
	await chat.set_sampler_config(regex_cfg)

	var stream = chat.call("ask", "Please tell me what the capital city of Denmark is.")
	var response: String = await stream.call("completed")
	if response == "zqxjvkw":
		runner.ok("sampler_constraints: regex literal forced through top_k(1)")
	else:
		runner.fail("sampler_constraints: expected the constrained literal 'zqxjvkw', got: %s" % response)

	# JSON schema constraint (a Dictionary schema exercises the
	# variant_to_json conversion path; a JSON string is also accepted).
	await chat.reset_history()
	var schema := {
		"type": "object",
		"properties": {"capital": {"type": "string"}},
		"required": ["capital"],
		"additionalProperties": false,
	}
	var schema_cfg: Variant = NobodyWhoSamplerBuilder.new().json_schema(schema).temperature(0.8).dist()
	await chat.set_sampler_config(schema_cfg)

	stream = chat.call("ask", "Give me the capital of Denmark as JSON with a 'capital' field.")
	var json_response: String = await stream.call("completed")
	var parsed = JSON.parse_string(json_response)
	if parsed is Dictionary and parsed.has("capital"):
		runner.ok("sampler_constraints: json_schema gave valid JSON with a 'capital' field")
	else:
		runner.fail("sampler_constraints: response was not the constrained JSON, got: %s" % json_response)

	# Don't leak the constraint or prompt into the tests that run after this.
	await chat.set_sampler_config(NobodyWhoSamplerPresets.default())
	await chat.reset_history()
	await chat.set_system_prompt(null)
	await chat.set_template_variable("enable_thinking", true)

func _test_complete(runner: Node, chat) -> void:
	# A leading system message becomes the system prompt; the list is the
	# whole conversation, and the response is appended to it.
	await chat.reset_history()
	var stream = chat.complete([
		{"role": "system", "content": "You answer with a single word."},
		{"role": "user", "content": "What is the capital of France?"},
	], {})
	if stream == null:
		runner.fail("complete: returned null for a valid message list")
		return
	var response: String = await stream.completed()
	if response.to_lower().find("paris") < 0:
		runner.fail("complete: expected Paris in the answer, got: %s" % response)
	else:
		runner.ok("complete: answered from the message list")

	# The passed list became the history — the leading system message is
	# consumed into the chat's system prompt, leaving [user, assistant].
	var hist = await chat.get_chat_history()
	var prompt: String = await chat.get_system_prompt()
	var roles_ok: bool = hist is Array and hist.size() == 2 and hist[0]["role"] == "user" and hist[1]["role"] == "assistant"
	if roles_ok and hist[1]["content"].to_lower().find("paris") >= 0 and prompt == "You answer with a single word.":
		runner.ok("complete: the list became the chat history (system message became the prompt)")
	else:
		runner.fail("complete: history wrong after complete (prompt=%s): %s" % [prompt, str(hist)])

	# Per-turn settings: pass a sampler, it stays set after the turn.
	stream = chat.complete(
		[{"role": "user", "content": "Name one fruit."}],
		{"sampler": NobodyWhoSamplerPresets.temperature(0.456)},
	)
	if stream == null:
		runner.fail("complete: per-turn settings rejected")
		return
	var _text: String = await stream.completed()
	var cfg = await chat.get_sampler_config()
	var cfg_json: String = cfg.to_json() if cfg else ""
	if cfg and cfg_json.find("0.456") >= 0:
		runner.ok("complete: per-turn sampler stays set after the turn")
	else:
		runner.fail("complete: per-turn sampler did not stick, got: %s" % cfg_json)

	# An empty list is rejected (the godot_error! is the expected noise).
	await chat.reset_history()
	var bad = chat.complete([], {})
	if bad == null:
		runner.ok("complete: rejects an empty message list")
	else:
		runner.fail("complete: empty list should return null")

	# Restore the default sampler so later tests aren't affected.
	await chat.set_sampler_config(NobodyWhoSamplerPresets.default())
	await chat.reset_history()
	await chat.set_system_prompt(null)

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
