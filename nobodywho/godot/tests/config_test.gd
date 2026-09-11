extends Node
# Model-less validation tests. Every call fails before loading a model.

func run(runner: Node) -> void:
	_expect_null(
		runner,
		NobodyWhoModel.create("unused.gguf", {"use_gup": true}),
		"config: model rejects unknown keys",
	)
	_expect_null(
		runner,
		NobodyWhoChat.create("unused.gguf", {"n_ctx": "4096"}),
		"config: chat rejects wrong value types",
	)
	_expect_null(
		runner,
		NobodyWhoChat.create("unused.gguf", {"n_ctx": 0}),
		"config: chat rejects zero n_ctx",
	)
	_expect_null(
		runner,
		NobodyWhoEncoder.create("unused.gguf", {"n_ctx": -1}),
		"config: encoder rejects negative n_ctx",
	)
	_expect_null(
		runner,
		NobodyWhoCrossEncoder.create("unused.gguf", {"n_ctx": 4294967296}),
		"config: crossencoder rejects overflowing n_ctx",
	)
	_expect_null(
		runner,
		NobodyWhoSpeechToText.create("unused", {"quantization": "q5"}),
		"config: speech-to-text rejects unknown quantization",
	)
	_expect_null(
		runner,
		NobodyWhoTextToSpeech.create("unused", {"architecture": "kokoro", "speed": -1.0}),
		"config: text-to-speech rejects negative speed",
	)
	_expect_null(
		runner,
		NobodyWhoTextToSpeech.create("unused", {"architecture": "kokoro", "steps": 1}),
		"config: text-to-speech rejects architecture-incompatible keys",
	)
	_expect_null(
		runner,
		NobodyWhoVoiceActivityDetection.create("unused", {"sample_rate": 0}),
		"config: VAD rejects zero sample rate",
	)
	_expect_null(
		runner,
		NobodyWhoVoiceActivityDetection.create("unused", {"threshold": 1.1}),
		"config: VAD rejects out-of-range thresholds",
	)
	_expect_null(
		runner,
		NobodyWhoVoiceActivityDetection.create("unused", {42: true}),
		"config: rejects non-String keys",
	)

func _expect_null(runner: Node, value: Variant, message: String) -> void:
	if value == null:
		runner.ok(message)
	else:
		runner.fail("%s (got %s)" % [message, str(value)])
