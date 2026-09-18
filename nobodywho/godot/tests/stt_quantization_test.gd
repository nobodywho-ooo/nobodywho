extends Node
# Tests that every `quantization` value documented in
# docs/docs-godot/speech-to-text.md (default, fp32, int8, uint8, bnb4, q4,
# quantized) actually works end-to-end, plus the documented
# "falls back to default when the repo doesn't ship a q4 variant" behavior.
#
# Env vars:
#   TEST_QUANT_SOURCE     - source under test (defaults to TEST_STT_SOURCE).
#   TEST_AUDIO_FILE       - audio file with known speech.
#   TEST_QUANT_ONLY       - optional comma-separated subset of values to test
#                           (e.g. "q4"), instead of all documented ones.

# Exactly the values listed in docs/docs-godot/speech-to-text.md.
const DOCUMENTED_VALUES := [
	"default", "fp32", "int8", "uint8", "bnb4", "q4", "quantized",
]

func run(runner: Node) -> void:
	var source: String = OS.get_environment("TEST_QUANT_SOURCE")
	if source.is_empty():
		source = OS.get_environment("TEST_STT_SOURCE")
	var audio: String = OS.get_environment("TEST_AUDIO_FILE")
	if source.is_empty() or audio.is_empty():
		print("SKIP: stt_quantization_test needs TEST_QUANT_SOURCE (or TEST_STT_SOURCE) and TEST_AUDIO_FILE")
		return

	# Validation runs before any source I/O, so any source string works here.
	await _test_invalid_value_rejected(runner, source)
	await _test_documented_values(runner, source, audio)

func _test_invalid_value_rejected(runner: Node, source: String) -> void:
	# Unknown values (and the unsupported fp16/q4f16) must be rejected cleanly
	# (null, no hang/crash), not silently accepted or fatal.
	for q in ["not-a-real-quantization", "fp16", "q4f16"]:
		var stt = await NobodyWhoSpeechToText.create(source, {"quantization": q})
		if stt == null:
			runner.ok("stt-quant: quantization '%s' rejected cleanly (null)" % q)
		else:
			runner.fail("stt-quant: quantization '%s' was accepted" % q)

func _test_documented_values(runner: Node, source: String, audio: String) -> void:
	var only: String = OS.get_environment("TEST_QUANT_ONLY")
	var values: Array = DOCUMENTED_VALUES if only.is_empty() else only.split(",", false)
	for q in values:
		var stt = await NobodyWhoSpeechToText.create(source, {
			"quantization": q,
			"language": "en",
		})
		if stt == null:
			runner.fail("stt-quant: documented value '%s' -> create() failed" % q)
			continue
		var text: String = await stt.transcribe_file(audio)
		if text == null or text.is_empty():
			runner.fail("stt-quant: '%s' created but transcribe_file returned empty/null" % q)
		else:
			runner.ok("stt-quant: '%s' works (transcript: '%s')" % [q, text.substr(0, 60).replace("\n", " ")])
