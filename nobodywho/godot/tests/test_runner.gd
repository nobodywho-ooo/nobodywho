extends Node
# Runs every NobodyWho Godot test suite headlessly, then exits.
# Exit code: 0 = all pass, 1 = at least one failure.
#
# Add a new suite by creating a `*_test.gd` Node script with an async
# `run(runner)` method, then append a `preload(...).new()` entry below.

var _failures: int = 0

func _ready() -> void:
	await _run_all()
	var code := 1 if _failures > 0 else 0
	print("\n=== tests done: %d failure(s) ===" % _failures)
	get_tree().quit(code)

func _run_all() -> void:
	var suites: Array = [
		preload("res://chat_test.gd").new(),
		preload("res://tools_test.gd").new(),
	]
	for suite in suites:
		await suite.run(self)

func ok(msg: String) -> void:
	print("PASS: ", msg)

func fail(msg: String) -> void:
	_failures += 1
	push_error("FAIL: " + msg)
	print("FAIL: ", msg)
