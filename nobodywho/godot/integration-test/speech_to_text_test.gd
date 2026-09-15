extends Node

# Smoke test for NobodyWhoSpeechToText.
# Uses hf://onnx-community/whisper-base from HuggingFace (downloaded and cached on first run).
# The test audio says "Hey Ron. Hey Billy."
#
# TEST_AUDIO_FILE env var overrides the audio path; defaults to the shared asset
# committed alongside the Python tests.

const WHISPER_MODEL := "hf://onnx-community/whisper-base"
# Shared test asset in assets/ — relative to the Godot project root.
const AUDIO_PATH := "res://../../../assets/sound.mp3"


func run_test() -> bool:
	print("🎙️ Starting stt_test")

	var audio_path := OS.get_environment("TEST_AUDIO_FILE")
	if audio_path.is_empty():
		audio_path = ProjectSettings.globalize_path(AUDIO_PATH)

	var stt := NobodyWhoSpeechToText.new()
	stt.model_path = WHISPER_MODEL
	# Use fp32 ("default"): the q4 whisper-base encoder mis-transcribes
	# "Billy" as "Bailey", while fp32 gets it right.
	stt.quantization = "default"
	add_child(stt)

	stt.worker_failed.connect(func(err: String):
		push_error("❌ stt_test worker_failed: " + err)
		get_tree().quit(1)
	)

	stt.start_worker()
	await stt.worker_started

	var pieces: Array[String] = []
	stt.transcription_updated.connect(func(piece: String): pieces.append(piece))

	stt.transcribe_file(audio_path)
	var full: String = await stt.transcription_finished

	var streamed := "".join(pieces)
	print("✨ stt_test transcript: " + full)
	print("✨ stt_test streamed %d pieces: %s" % [pieces.size(), streamed])

	assert(not pieces.is_empty(), "Expected transcription_updated to emit at least one piece")
	assert(
		streamed.strip_edges() == full.strip_edges(),
		"Streamed pieces '%s' do not match final transcript '%s'" % [streamed, full]
	)

	assert("ron" in full.to_lower(), "Expected 'ron' in transcript, got: " + full)
	assert("billy" in full.to_lower(), "Expected 'billy' in transcript, got: " + full)

	return true
