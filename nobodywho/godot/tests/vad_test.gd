extends Node
# Tests for NobodyWhoVoiceActivityDetection. Model-backed (needs TEST_VAD_MODEL
# pointing at a Silero VAD layout — a local dir with onnx/model.onnx).
#
# Uses the shared test asset assets/sound_16k.wav ("Hey Ron. Hey Billy.").
# The default threshold/min_speech_duration_ms combo is tuned for typical
# mic-level speech and doesn't confirm speech_started on this quiet clip, so
# the detection tests loosen both — same as python's test_vad.py.
#
# Run: TEST_VAD_MODEL=/path/to/silero \
#      nix shell nixpkgs#godot_4 --command godot --headless --path .

var _model: String
var _wav: String

func run(runner: Node) -> void:
	_model = OS.get_environment("TEST_VAD_MODEL")
	_wav = OS.get_environment("TEST_AUDIO_FILE_WAV")
	if _model.is_empty() or _wav.is_empty():
		print("SKIP: vad_test needs TEST_VAD_MODEL and TEST_AUDIO_FILE_WAV env vars")
		return

	var audio := _read_wav_mono_i16(_wav)
	if audio.is_empty():
		runner.fail("vad: could not read %s as a PCM WAV" % _wav)
		return

	await _test_push_and_finish(runner, audio)
	await _test_silence(runner)
	await _test_segment(runner, audio)

func _make_vad(config: Dictionary) -> Variant:
	return await NobodyWhoVoiceActivityDetection.create(_model, config)

func _test_push_and_finish(runner: Node, audio: Dictionary) -> void:
	var vad = await _make_vad({
		"sample_rate": audio["sample_rate"],
		"threshold": 0.3,
		"min_speech_duration_ms": 90,
	})
	if vad == null:
		runner.fail("vad: push: could not create VAD (check TEST_VAD_MODEL)")
		return

	var samples: PackedByteArray = audio["samples"]
	var started := false
	var ended := false
	var chunk_bytes := 800 * 2  # 800 samples, LE i16
	var i := 0
	while i < samples.size() and not ended:
		var event: String = vad.push(samples.slice(i, i + chunk_bytes))
		if event == null:
			runner.fail("vad: push returned null (inference error)")
			return
		if event == "speech_started":
			started = true
		elif event == "speech_ended":
			ended = true
		i += chunk_bytes

	if not started:
		runner.fail("vad: push: speech_started never fired on real speech audio")
		return
	if not ended:
		runner.fail("vad: push: speech_ended never fired after the speech stops")
		return

	var turn: PackedByteArray = vad.finish()
	if turn.size() > 0 and turn.size() < samples.size():
		runner.ok("vad: push detected the turn (%d of %d bytes), finish() returned it" % [turn.size(), samples.size()])
	else:
		runner.fail("vad: finish returned %d bytes (expected 0 < n < %d)" % [turn.size(), samples.size()])

	vad = null

func _test_silence(runner: Node) -> void:
	var vad = await _make_vad({"sample_rate": 16000})
	if vad == null:
		runner.fail("vad: silence: could not create VAD")
		return

	var zeros := PackedByteArray()
	zeros.resize(512 * 2)
	for i in range(5):
		var event: String = vad.push(zeros)
		if event != "silence":
			runner.fail("vad: silence: expected 'silence', got %s" % str(event))
			return
	if vad.finish().size() != 0:
		runner.fail("vad: silence: finish() should be empty when speech was never confirmed")
	else:
		runner.ok("vad: silence stays silence, finish() is empty")

	vad = null

func _test_segment(runner: Node, audio: Dictionary) -> void:
	var vad = await _make_vad({
		"sample_rate": audio["sample_rate"],
		"threshold": 0.3,
		"min_speech_duration_ms": 90,
	})
	if vad == null:
		runner.fail("vad: segment: could not create VAD")
		return

	var segments = vad.segment(audio["samples"])
	if segments == null:
		runner.fail("vad: segment returned null (inference error)")
		return
	if segments.is_empty():
		runner.fail("vad: segment found no speech in the recording")
		return
	var total: int = audio["samples"].size()
	for s in segments:
		if s.size() == 0 or s.size() >= total:
			runner.fail("vad: segment has bad size %d (of %d)" % [s.size(), total])
			return
	runner.ok("vad: segment found %d speech segment(s)" % segments.size())

	vad = null

# --- test helpers -----------------------------------------------------------

## Parse a PCM WAV into mono LE-i16 samples. Downmixes multi-channel by
## averaging channels per frame (mirrors python's _read_wav_mono_i16).
func _read_wav_mono_i16(path: String) -> Dictionary:
	var bytes := FileAccess.get_file_as_bytes(path)
	if bytes.size() < 12:
		push_error("vad_test: %s is too short to be a WAV" % path)
		return {}
	if bytes.slice(0, 4).get_string_from_ascii() != "RIFF" or bytes.slice(8, 12).get_string_from_ascii() != "WAVE":
		push_error("vad_test: %s is not a RIFF/WAVE file" % path)
		return {}

	var channels := 0
	var sample_rate := 0
	var data := PackedByteArray()
	var pos := 12
	while pos + 8 <= bytes.size():
		var id := bytes.slice(pos, pos + 4).get_string_from_ascii()
		var size := bytes.decode_u32(pos + 4)
		var chunk := bytes.slice(pos + 8, pos + 8 + size)
		if id == "fmt ":
			channels = chunk.decode_u16(2)
			sample_rate = chunk.decode_u32(4)
		elif id == "data":
			data = chunk
		pos += 8 + size + (size & 1)  # chunks are padded to even sizes

	if channels == 0 or sample_rate == 0 or data.is_empty():
		push_error("vad_test: %s lacks a usable fmt/data chunk" % path)
		return {}

	var frames := data.size() / 2 / channels
	var out := PackedByteArray()
	out.resize(frames * 2)
	for i in frames:
		var acc := 0
		for c in channels:
			acc += data.decode_s16((i * channels + c) * 2)
		out.encode_s16(i * 2, acc / channels)
	return {"samples": out, "sample_rate": sample_rate}
