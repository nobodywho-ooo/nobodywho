extends Node
# Tests for NobodyWhoTask — the latched async-result primitive that everything
# else in the rewrite builds on. Exercises the four latch cases plus the
# blocking-thread panic path from REWRITE_PLAN.md §10.

func run(runner: Node) -> void:
	# 1. resolve-before-await: instant task, let a frame pass, then await.
	#    The latch must hand back the value after completion.
	var t1 := NobodyWhoTask._test_instant("hello")
	await runner.get_tree().process_frame
	var v1 = await t1.wait()
	if v1 == "hello":
		runner.ok("task: resolve-before-await")
	else:
		runner.fail("task: resolve-before-await: expected 'hello', got %s" % str(v1))

	# 2. resolve-after-await: delayed task, await before it completes.
	var t2 := NobodyWhoTask._test_delay(40, "world")
	var v2 = await t2.wait()
	if v2 == "world":
		runner.ok("task: resolve-after-await")
	else:
		runner.fail("task: resolve-after-await: expected 'world', got %s" % str(v2))

	# 3. double-await: same instant task awaited twice, both return the value.
	var t3 := NobodyWhoTask._test_instant("dbl")
	var a3 = await t3.wait()
	var b3 = await t3.wait()
	if a3 == "dbl" and b3 == "dbl":
		runner.ok("task: double-await")
	else:
		runner.fail("task: double-await: got %s then %s" % [str(a3), str(b3)])

	# 4. never-await: create a delayed task and never await it. It must still
	#    complete, latch, and not hang the process.
	var t4 := NobodyWhoTask._test_delay(10, "never")
	await runner.get_tree().create_timer(0.08).timeout
	if not t4.is_done():
		runner.fail("task: never-await: should be done without an awaiter")
	elif t4.result() != "never":
		runner.fail("task: never-await: latched result should be 'never', got %s" % str(t4.result()))
	else:
		runner.ok("task: never-await")

	# 5. blocking panic: a closure that panics on the blocking thread must
	#    resolve to null (with a godot_error), not hang or abort.
	var t5 := NobodyWhoTask._test_blocking_panic()
	var v5 = await t5.wait()
	if v5 == null:
		runner.ok("task: blocking-panic -> null, no hang")
	else:
		runner.fail("task: blocking-panic: expected null, got %s" % str(v5))
