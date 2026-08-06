extends Node
# Tests for NobodyWhoEncoder. Model-backed (needs TEST_ENCODER_MODEL).
#
# Run: TEST_ENCODER_MODEL=/path/to/bge-small-en-v1.5-q8_0.gguf \
#      nix shell nixpkgs#godot_4 --command godot --headless --path .

var _model: String

func run(runner: Node) -> void:
	_model = OS.get_environment("TEST_ENCODER_MODEL")
	if _model.is_empty():
		print("SKIP: encoder_test needs TEST_ENCODER_MODEL env var (e.g. bge-small-en-v1.5-q8_0.gguf)")
		return
	await _test_encode(runner)
	await _test_encode_batch(runner)
	await _test_cosine_similarity(runner)

func _test_encode(runner: Node) -> void:
	var enc = await NobodyWhoEncoder.create(_model, {})
	if enc == null:
		runner.fail("embedding: could not create encoder (check TEST_ENCODER_MODEL)")
		return
	var vec: PackedFloat32Array = await enc.encode("What is the weather like?")
	if vec == null or vec.size() == 0:
		runner.fail("embedding: encode returned empty/null")
		return
	runner.ok("embedding: encode returned a %d-dim vector" % vec.size())
	enc = null

func _test_encode_batch(runner: Node) -> void:
	var enc = await NobodyWhoEncoder.create(_model, {})
	if enc == null:
		runner.fail("embedding: batch: could not create encoder")
		return
	var vecs: Array = await enc.encode_batch(["Paris is the capital of France.", "Berlin is the capital of Germany."])
	if vecs == null or vecs.size() != 2:
		runner.fail("embedding: batch: expected 2 vectors, got %s" % str(vecs))
		return
	var v0: PackedFloat32Array = vecs[0]
	var v1: PackedFloat32Array = vecs[1]
	if v0.size() == 0 or v0.size() != v1.size():
		runner.fail("embedding: batch: vectors empty or mismatched (v0=%d v1=%d)" % [v0.size(), v1.size()])
		return
	runner.ok("embedding: encode_batch returned 2 vectors of %d dims" % v0.size())
	enc = null

func _test_cosine_similarity(runner: Node) -> void:
	# Pure math — no model needed for this assertion, but we reuse the encoder
	# to get real vectors.
	var enc = await NobodyWhoEncoder.create(_model, {})
	if enc == null:
		runner.fail("embedding: cosine: could not create encoder")
		return
	var a: PackedFloat32Array = await enc.encode("hello world")
	var b: PackedFloat32Array = await enc.encode("hello world")
	if a == null or a.size() == 0:
		runner.fail("embedding: cosine: could not encode")
		return
	var sim: float = NobodyWhoEncoder.cosine_similarity(a, b)
	# Identical strings → cosine similarity ≈ 1.0 (within float precision).
	if abs(sim - 1.0) < 0.001:
		runner.ok("embedding: cosine_similarity(a, a) = %.4f (≈1.0)" % sim)
	else:
		runner.fail("embedding: cosine_similarity(a, a) = %.4f, expected ≈1.0" % sim)
	enc = null
