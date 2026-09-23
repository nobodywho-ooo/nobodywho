extends Node
# Tests for NobodyWhoSpeechToText. Model-backed (needs TEST_STT_SOURCE and
# TEST_AUDIO_FILE pointing at a .wav/.mp3 with known speech).
#
# Run: TEST_STT_SOURCE=hf://onnx-community/whisper-base \
#      TEST_AUDIO_FILE=/path/to/hello.wav \
#      nix shell nixpkgs#godot_4 --command godot --headless --path .

var _source: String
var _audio: String

func run(runner: Node) -> void:
	_source = OS.get_environment("TEST_STT_SOURCE")
	_audio = OS.get_environment("TEST_AUDIO_FILE")
	if _source.is_empty() or _audio.is_empty():
		print("SKIP: stt_test needs TEST_STT_SOURCE and TEST_AUDIO_FILE env vars")
		return
	await _test_transcribe_file(runner)
	await _test_transcribe_file_stream(runner)

func _test_transcribe_file(runner: Node) -> void:
	var stt = await NobodyWhoSpeechToText.create(_source, {})
	if stt == null:
		runner.fail("stt: could not create SpeechToText (check TEST_STT_SOURCE)")
		return

	var text: String = await stt.transcribe_file(_audio)
	if text == null or text.is_empty():
		runner.fail("stt: transcribe_file returned empty/null")
		return
	runner.ok("stt: transcribe_file returned '%s'" % text.substr(0, 40))

	stt = null

func _test_transcribe_file_stream(runner: Node) -> void:
	var stt = await NobodyWhoSpeechToText.create(_source, {})
	if stt == null:
		runner.fail("stt: stream: could not create SpeechToText")
		return

	var stream = stt.transcribe_file_stream(_audio)
	if stream == null:
		runner.fail("stt: stream: transcribe_file_stream returned null")
		return

	var collected := ""
	while true:
		var piece = await stream.next_token()
		if piece == null:
			break
		collected += piece
	var full: String = await stream.completed()
	if collected.is_empty() and full.is_empty():
		runner.fail("stt: stream: both pull loop and completed() empty")
	elif full.is_empty():
		runner.fail("stt: stream: pull loop got '%s' but completed() empty" % collected.substr(0, 40))
	else:
		runner.ok("stt: stream pull + completed() = '%s'" % full.substr(0, 40))

	stt = null
