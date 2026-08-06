extends Node
# Tests for NobodyWhoTextToSpeech. Model-backed (needs TEST_TTS_SOURCE).
#
# Run: TEST_TTS_SOURCE=hf://NobodyWho/Kokoro-82M \
#      nix shell nixpkgs#godot_4 --command godot --headless --path .

var _source: String

func run(runner: Node) -> void:
	_source = OS.get_environment("TEST_TTS_SOURCE")
	if _source.is_empty():
		print("SKIP: tts_test needs TEST_TTS_SOURCE env var (e.g. hf://NobodyWho/Kokoro-82M)")
		return
	await _test_synthesize(runner)

func _test_synthesize(runner: Node) -> void:
	var tts = await NobodyWhoTextToSpeech.create(_source, {})
	if tts == null:
		runner.fail("tts: could not create TextToSpeech (check TEST_TTS_SOURCE)")
		return

	var wav: PackedByteArray = await tts.synthesize("Hello from NobodyWho.")
	if wav == null or wav.size() == 0:
		runner.fail("tts: synthesize returned empty/null")
		return
	# A WAV container starts with "RIFF" and has "WAVE" at offset 8.
	if wav.size() < 12 or wav.slice(0, 4) != "RIFF".to_ascii_buffer() or wav.slice(8, 12) != "WAVE".to_ascii_buffer():
		runner.fail("tts: synthesize did not return a WAV container (first bytes: %s)" % str(wav.slice(0, 12)))
		return
	runner.ok("tts: synthesize returned a %d-byte WAV container" % wav.size())

	tts = null
