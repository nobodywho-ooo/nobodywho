extends Node

# Smoke test for NobodyWhoSTT.
# Uses onnx-community/whisper-base from HuggingFace (downloaded and cached on first run).
# The test audio says "Hey Ron. Hey Billy."
#
# TEST_AUDIO_FILE env var overrides the audio path; defaults to the shared asset
# committed alongside the Python tests.

const WHISPER_MODEL := "onnx-community/whisper-base"
# Shared test asset in assets/ — relative to the Godot project root.
const AUDIO_PATH := "res://../../../assets/sound.mp3"


func run_test() -> bool:
	print("🎙️ Starting stt_test")

	var audio_path := ProjectSettings.globalize_path(AUDIO_PATH)

	var stt := NobodyWhoSTT.new()
	stt.model_path = WHISPER_MODEL
	add_child(stt)

	stt.worker_failed.connect(func(err: String):
		push_error("❌ stt_test worker_failed: " + err)
		get_tree().quit(1)
	)

	stt.start_worker()
	await stt.worker_started

	var transcript := ""
	stt.transcription_updated.connect(func(piece: String): transcript += piece)

	stt.transcribe_file(audio_path)
	var full: String = await stt.transcription_finished

	print("✨ stt_test transcript: " + full)
	assert("ron" in full.to_lower(), "Expected 'ron' in transcript, got: " + full)
	assert("billy" in full.to_lower(), "Expected 'billy' in transcript, got: " + full)

	return true
