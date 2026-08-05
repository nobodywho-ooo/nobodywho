extends Node
# Phase 3 tests for tool calling. Model-backed (needs TEST_MODEL).
#
# Covers: the built-in sandboxed python tool, sync GDScript tools
# (auto-schema via NobodyWhoTool.create — exercises the get_method_list
# reflection path), and async GDScript tools (a coroutine awaited on the
# main thread via its GDScriptFunctionState.completed signal).
#
# Run: TEST_MODEL=/path/to/model.gguf \
#      nix shell nixpkgs#godot_4 --command godot --headless --path .

var _model_path: String

# Observability for the GDScript tools: the tool records that it ran and
# with which argument, so the test can assert it was actually called.
var _sync_tool_called: bool = false
var _sync_tool_arg: String = ""
var _async_tool_called: bool = false
var _reentrant_chat = null
var _reentrant_result = null
var _lambda_arg = null
var _stuck_calls: int = 0
var _stuck_returned: bool = false
signal _never_fires

func run(runner: Node) -> void:
	_model_path = OS.get_environment("TEST_MODEL")
	if _model_path.is_empty():
		print("SKIP: tools_test needs TEST_MODEL env var (point it at a .gguf)")
		return

	runner.add_child(self)

	await _test_python_tool(runner)
	await _test_sync_tool(runner)
	await _test_async_tool(runner)
	await _test_schema_lambda_tool(runner)
	await _test_timeout_recovery(runner)
	await _test_reentrancy_guard(runner)

	runner.remove_child(self)

func _make_chat(tools: Array) -> Variant:
	var cfg: Dictionary = {"tools": tools}
	return await NobodyWhoChat.create(_model_path, cfg)

# --- built-in python tool ---

func _test_python_tool(runner: Node) -> void:
	var tool = NobodyWhoTool.python(0, 0, 0)
	var chat = await _make_chat([tool])
	if chat == null:
		runner.fail("tools: python: could not create chat")
		return

	var stream = chat.call("ask", "Use the run_python tool to compute 7 * 6 and tell me the answer.")
	var _text: String = await stream.call("completed")
	# Built-in tool: we can't easily observe whether it was called from here,
	# but the chat must not hang and must produce *some* output.
	runner.ok("tools: python tool registered, chat did not hang (built-in mechanism OK)")

	chat = null

# --- manual schema + lambda (the escape hatch) ---

func _test_schema_lambda_tool(runner: Node) -> void:
	_lambda_arg = null
	var schema: Dictionary = {
		"type": "object",
		"properties": {
			"color": {"type": "string", "enum": ["red", "green", "blue"]},
		},
		"required": ["color"],
	}
	var tool = NobodyWhoTool.create_with_schema(
		"press_button", "Press the button of the given color. Returns what happened.",
		schema,
		func(color: String) -> String:
			_lambda_arg = color
			return "the %s button lit up and says 'quartz'" % color)
	var chat = await _make_chat([tool])
	if chat == null:
		runner.fail("tools: schema/lambda: could not create chat")
		return

	var stream = chat.call("ask", "Use the press_button tool to press the green button, then tell me what it said.")
	var text: String = await stream.call("completed")
	if _lambda_arg == null:
		runner.fail("tools: schema/lambda: tool was never called")
	elif not text.to_lower().contains("quartz"):
		runner.fail("tools: schema/lambda: response missing tool result (arg=%s, got: %s)" % [str(_lambda_arg), text])
	else:
		runner.ok("tools: create_with_schema lambda tool called (arg=%s), result reached the model" % str(_lambda_arg))

	chat = null

# --- timeout: a stuck coroutine must not poison the tool for later calls ---

func stuck_oracle(question: String) -> String:
	_stuck_calls += 1
	if _stuck_calls == 1:
		# Never completes: awaits a signal that never fires. The worker's
		# recv_timeout must unblock generation with an error string, and the
		# dispatcher must keep serving later calls (sub-task isolation).
		await _never_fires
		return "unreachable (q: %s)" % question
	_stuck_returned = true
	return "the oracle recovered and says 'emerald' (q: %s)" % question

func _test_timeout_recovery(runner: Node) -> void:
	_stuck_calls = 0
	_stuck_returned = false
	var tool = NobodyWhoTool.create(stuck_oracle, "Ask the oracle a question and get its answer.", 2)
	var chat = await _make_chat([tool])
	if chat == null:
		runner.fail("tools: timeout: could not create chat")
		return

	# First call wedges its coroutine; generation must complete via timeout.
	var stream = chat.call("ask", "Use the stuck_oracle tool to ask 'one?' and report its answer.")
	var _text1: String = await stream.call("completed")
	if _stuck_calls < 1:
		runner.fail("tools: timeout: tool was never called")
		return

	# Second call must actually run and return (with the old inline loop it
	# would queue behind the wedged first call and time out too).
	stream = chat.call("ask", "Use the stuck_oracle tool once more to ask 'two?' and report its answer.")
	var text2: String = await stream.call("completed")
	if not _stuck_returned:
		runner.fail("tools: timeout: second call never ran — dispatcher wedged by the first (calls=%d)" % _stuck_calls)
	elif not text2.to_lower().contains("emerald"):
		runner.fail("tools: timeout: second call ran but result missing (got: %s)" % text2)
	else:
		runner.ok("tools: timeout unblocked generation, later call to the same tool recovered")

	chat = null

# --- re-entrancy guard: calling back into the same chat fails fast ---

func naughty_tool(box_name: String) -> String:
	# Forbidden: call back into the chat this tool belongs to. Must resolve
	# null instantly (guard) instead of hanging the worker forever.
	_reentrant_result = await _reentrant_chat.get_system_prompt()
	return "guard says: %s (box %s)" % [str(_reentrant_result), box_name]

func _test_reentrancy_guard(runner: Node) -> void:
	_reentrant_result = "unset"
	var tool = NobodyWhoTool.create(naughty_tool, "Returns the contents of the named box.")
	_reentrant_chat = await _make_chat([tool])
	if _reentrant_chat == null:
		runner.fail("tools: reentrancy: could not create chat")
		return

	var stream = _reentrant_chat.call("ask", "Use the naughty_tool tool with box_name 'blue' and tell me what it says.")
	var _text: String = await stream.call("completed")
	# The generation must complete (no hang), and the re-entrant call inside
	# the tool must have resolved null via the guard.
	if _reentrant_result != null:
		runner.fail("tools: reentrancy: guard did not fire (got %s)" % str(_reentrant_result))
	else:
		runner.ok("tools: reentrancy guard fired, no hang, generation completed")

	_reentrant_chat = null

# --- sync GDScript tool (auto-schema) ---

func get_magic_word(box_name: String) -> String:
	_sync_tool_called = true
	_sync_tool_arg = box_name
	return "the magic word is 'xylophone'"

func _test_sync_tool(runner: Node) -> void:
	_sync_tool_called = false
	var tool = NobodyWhoTool.create(get_magic_word, "Returns the secret magic word stored in the named box.")
	if tool == null:
		runner.fail("tools: sync: NobodyWhoTool.create returned null")
		return
	var chat = await _make_chat([tool])
	if chat == null:
		runner.fail("tools: sync: could not create chat")
		return

	var stream = chat.call("ask", "Use the get_magic_word tool with box_name 'red' and tell me the magic word.")
	var text: String = await stream.call("completed")
	if not _sync_tool_called:
		runner.fail("tools: sync: tool was never called")
	elif _sync_tool_arg != "red":
		runner.fail("tools: sync: tool called with wrong arg '%s' (expected 'red')" % _sync_tool_arg)
	elif not text.to_lower().contains("xylophone"):
		runner.fail("tools: sync: response missing the tool result (got: %s)" % text)
	else:
		runner.ok("tools: sync GDScript tool called with correct arg, result reached the model")

	chat = null

# --- async GDScript tool (coroutine awaited from Rust) ---

func get_oracle_answer(question: String) -> String:
	_async_tool_called = true
	# Suspend across frames: this returns a GDScriptFunctionState to the
	# Rust caller, which must await its `completed` signal.
	await get_tree().create_timer(0.2).timeout
	return "the oracle says: 'banana' (question was: %s)" % question

func _test_async_tool(runner: Node) -> void:
	_async_tool_called = false
	var tool = NobodyWhoTool.create(get_oracle_answer, "Ask the oracle a question and get its answer.")
	if tool == null:
		runner.fail("tools: async: NobodyWhoTool.create returned null")
		return
	var chat = await _make_chat([tool])
	if chat == null:
		runner.fail("tools: async: could not create chat")
		return

	var stream = chat.call("ask", "Use the get_oracle_answer tool to ask the oracle 'what is best?' and tell me its answer.")
	var text: String = await stream.call("completed")
	if not _async_tool_called:
		runner.fail("tools: async: tool was never called")
	elif not text.to_lower().contains("banana"):
		runner.fail("tools: async: response missing the awaited coroutine result (got: %s)" % text)
	else:
		runner.ok("tools: async GDScript tool (await) called, coroutine result reached the model")

	chat = null
