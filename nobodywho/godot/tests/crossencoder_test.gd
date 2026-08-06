extends Node
# Tests for NobodyWhoCrossEncoder. Model-backed (needs TEST_CROSSENCODER_MODEL).
#
# Run: TEST_CROSSENCODER_MODEL=/path/to/bge-reranker-v2-m3-Q8_0.gguf \
#      nix shell nixpkgs#godot_4 --command godot --headless --path .

var _model: String

func run(runner: Node) -> void:
	_model = OS.get_environment("TEST_CROSSENCODER_MODEL")
	if _model.is_empty():
		print("SKIP: crossencoder_test needs TEST_CROSSENCODER_MODEL env var (e.g. bge-reranker-v2-m3-Q8_0.gguf)")
		return
	await _test_rank(runner)
	await _test_rank_and_sort(runner)

func _test_rank(runner: Node) -> void:
	var ce = await NobodyWhoCrossEncoder.create(_model, {})
	if ce == null:
		runner.fail("crossencoder: could not create reranker (check TEST_CROSSENCODER_MODEL)")
		return
	var docs = [
		"Someone previously asked about Python packages",
		"Use pip install package-name to install Python packages.",
		"Python packages are not included in the standard library.",
	]
	var scores = await ce.rank("How do I install Python packages?", docs)
	if scores == null or scores.size() != 3:
		runner.fail("crossencoder: rank returned %s (expected 3 scores)" % str(scores))
		return
	# The binding mechanism works if we get one float score per doc.
	# Which doc scores highest is model-dependent (and this small reranker
	# may not always pick the "obvious" one).
	runner.ok("crossencoder: rank -> 3 scores [%.3f, %.3f, %.3f]" % [scores[0], scores[1], scores[2]])
	ce = null

func _test_rank_and_sort(runner: Node) -> void:
	var ce = await NobodyWhoCrossEncoder.create(_model, {})
	if ce == null:
		runner.fail("crossencoder: sort: could not create reranker")
		return
	var docs = [
		"Someone previously asked about Python packages",
		"Use pip install package-name to install Python packages.",
		"Python packages are not included in the standard library.",
	]
	var ranked = await ce.rank_and_sort("How do I install Python packages?", docs)
	if ranked == null or ranked.size() != 3:
		runner.fail("crossencoder: sort: expected 3 results, got %s" % str(ranked))
		return
	# Check the output is sorted by score descending.
	var s0 = float(ranked[0].get("score", 0.0))
	var s1 = float(ranked[1].get("score", 0.0))
	var s2 = float(ranked[2].get("score", 0.0))
	if s0 >= s1 and s1 >= s2:
		runner.ok("crossencoder: rank_and_sort -> sorted desc [%.3f, %.3f, %.3f]" % [s0, s1, s2])
	else:
		runner.fail("crossencoder: rank_and_sort -> not sorted desc [%.3f, %.3f, %.3f]" % [s0, s1, s2])
	ce = null
