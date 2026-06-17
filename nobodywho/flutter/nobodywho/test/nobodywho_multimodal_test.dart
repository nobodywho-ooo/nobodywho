@Timeout(Duration(seconds: 600))
import 'package:nobodywho/nobodywho.dart' as nobodywho;
import 'package:test/test.dart';
import 'dart:io';
import 'dart:typed_data';

/// Decode a 16-bit PCM WAV fixture to mono samples + sample rate.
///
/// Walks the RIFF chunk list to find `fmt ` and `data` — WAV writers sometimes
/// insert padding chunks (e.g. `FLLR`) between them, so we can't assume `data`
/// starts at byte 44.
({Int16List samples, int sampleRate}) readWavAsPcm(String path) {
  final bytes = File(path).readAsBytesSync();
  final bd = ByteData.sublistView(bytes);

  if (String.fromCharCodes(bytes.sublist(0, 4)) != 'RIFF' ||
      String.fromCharCodes(bytes.sublist(8, 12)) != 'WAVE') {
    throw StateError('not a RIFF/WAVE file: $path');
  }

  int? channels;
  int? sampleRate;
  int? bitsPerSample;
  Uint8List? pcmBytes;

  // Iterate chunks starting after the "WAVE" header.
  var offset = 12;
  while (offset + 8 <= bytes.length) {
    final id = String.fromCharCodes(bytes.sublist(offset, offset + 4));
    final size = bd.getUint32(offset + 4, Endian.little);
    final body = offset + 8;

    if (id == 'fmt ') {
      channels = bd.getUint16(body + 2, Endian.little);
      sampleRate = bd.getUint32(body + 4, Endian.little);
      bitsPerSample = bd.getUint16(body + 14, Endian.little);
    } else if (id == 'data') {
      pcmBytes = Uint8List.sublistView(bytes, body, body + size);
      break;
    }
    // Chunks are padded to an even size.
    offset = body + size + (size.isOdd ? 1 : 0);
  }

  if (channels == null || sampleRate == null || bitsPerSample == null) {
    throw StateError('missing fmt chunk in WAV: $path');
  }
  if (bitsPerSample != 16) {
    throw StateError('test WAV must be 16-bit PCM (got $bitsPerSample-bit)');
  }
  if (pcmBytes == null) {
    throw StateError('missing data chunk in WAV: $path');
  }

  final interleaved = Int16List.view(
    pcmBytes.buffer,
    pcmBytes.offsetInBytes,
    pcmBytes.lengthInBytes ~/ 2,
  );

  if (channels == 1) {
    return (samples: Int16List.fromList(interleaved), sampleRate: sampleRate);
  }
  // Downmix to mono.
  final mono = Int16List(interleaved.length ~/ channels);
  for (var i = 0; i < mono.length; i++) {
    var sum = 0;
    for (var c = 0; c < channels; c++) {
      sum += interleaved[i * channels + c];
    }
    mono[i] = sum ~/ channels;
  }
  return (samples: mono, sampleRate: sampleRate);
}

void main() {
  group('Multimodal tests', () {
    final modelPath = Platform.environment["TEST_MULTIMODAL_MODEL"];
    final mmprojPath = Platform.environment["TEST_MMPROJ_MODEL"];
    final imagePath = '${Directory.current.path}/test/dog.png';
    final audioPath = '${Directory.current.path}/test/sound_16k.wav';

    // Load the model once and share across tests — loading the vision model
    // multiple times in one Dart process exhausts memory and crashes the
    // test runner. Mirrors the python `multimodal_model` module-scoped fixture.
    late nobodywho.Model? sharedModel;

    setUpAll(() async {
      await nobodywho.NobodyWho.init();
      if (modelPath != null) {
        sharedModel = await nobodywho.Model.load(
          modelPath: modelPath,
          projectionModelPath: mmprojPath,
        );
      } else {
        sharedModel = null;
      }
    });

    nobodywho.Chat newChat({int contextSize = 4096}) => nobodywho.Chat(
          model: sharedModel!,
          systemPrompt: "",
          contextSize: contextSize,
          templateVariables: const {"enable_thinking": false},
          // Greedy sampling for deterministic transcription assertions.
          sampler: nobodywho.SamplerPresets.greedy(),
        );

    test('askWithPrompt with text only', () async {
      if (sharedModel == null) return;
      final chat = newChat(contextSize: 2048);
      final prompt = nobodywho.Prompt([
        nobodywho.TextPart("What is the capital of France?"),
      ]);
      final response = await chat.askWithPrompt(prompt).completed();
      expect(response, contains("Paris"));
    });

    test('askWithPrompt with image bytes', () async {
      if (sharedModel == null || mmprojPath == null) return;

      final bytes = await File(imagePath).readAsBytes();
      final chat = newChat();
      final prompt = nobodywho.Prompt([
        nobodywho.TextPart(
          "Describe what animal is in this image in one word. Do not focus on the age of the animal.",
        ),
        nobodywho.ImagePart.fromBytes(bytes),
      ]);

      final response = await chat.askWithPrompt(prompt).completed();
      expect(response.toLowerCase(), contains("dog"));
    });

    test('askWithPrompt with audio PCM', () async {
      if (sharedModel == null || mmprojPath == null) return;

      final wav = readWavAsPcm(audioPath);
      final chat = newChat();
      final prompt = nobodywho.Prompt([
        nobodywho.TextPart("Please transcribe this audio."),
        nobodywho.AudioPart.fromPcm(wav.samples, sampleRate: wav.sampleRate),
      ]);

      final response = await chat.askWithPrompt(prompt).completed();
      expect(response.toLowerCase(), contains("billy"));
    });

    test('askWithPrompt with image path (legacy)', () async {
      // Smoke test that the path-based constructors still work.
      if (sharedModel == null || mmprojPath == null) return;

      final chat = newChat();
      final prompt = nobodywho.Prompt([
        nobodywho.TextPart(
          "Describe what animal is in this image in one word. Do not focus on the age of the animal.",
        ),
        nobodywho.ImagePart(imagePath),
      ]);

      final response = await chat.askWithPrompt(prompt).completed();
      expect(response.toLowerCase(), contains("dog"));
    });
  });
}
