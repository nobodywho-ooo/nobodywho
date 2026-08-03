extends Node
# Tests for NobodyWhoTokenStream — the per-call pull stream returned by
# NobodyWhoChat.ask(). Uses the synthetic _test_stream so no model is needed;
# both the inline fast path (token already queued) and the suspend path
# (channel empty) get exercised.

func run(runner: Node) -> void:
	var arr: Array[String] = ["a", "b", "c"]
	var s := NobodyWhoTokenStream._test_stream(arr)

	var collected := ""
	while true:
		var tok = await s.next_token()
		if tok == null:
			break
		collected += tok
	var full = await s.completed()

	if collected == "abc":
		runner.ok("stream: pull loop (collected='%s')" % collected)
	else:
		runner.fail("stream: pull loop: expected 'abc', got '%s'" % collected)

	if full == "abc":
		runner.ok("stream: completed() latches full text")
	else:
		runner.fail("stream: completed(): expected 'abc', got '%s'" % str(full))
