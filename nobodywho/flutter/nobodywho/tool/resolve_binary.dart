#!/usr/bin/env dart

import 'dart:async';
import 'dart:io';

import 'package:args/args.dart';

// Applied to HttpClient.connectionTimeout and as an overall timeout on
// request.close(), so a stalled download fails fast instead of hanging.
const httpTimeout = Duration(seconds: 30);

// onnxruntime is a separate .so only on Android x86_64 (Microsoft ships no
// static build for it there); arm64 links it statically into the main lib.
// Keep onnxRuntimeVersion in sync with ORT_VERSION in .github/workflows/build.yml.
const onnxRuntimeVersion = '1.24.2';
const onnxRuntimeArches = {
  'android': ['x86_64'],
};

typedef Binary = ({String triple, String library});

const binaries = <String, Binary>{
  'linux-x86_64': (
    triple: 'x86_64-unknown-linux-gnu',
    library: 'libnobodywho_flutter.so',
  ),
  'linux-aarch64': (
    triple: 'aarch64-unknown-linux-gnu',
    library: 'libnobodywho_flutter.so',
  ),
  'windows-x86_64': (
    triple: 'x86_64-pc-windows-msvc',
    library: 'nobodywho_flutter.dll',
  ),
  'android-arm64-v8a': (
    triple: 'aarch64-linux-android',
    library: 'libnobodywho_flutter.so',
  ),
  'android-x86_64': (
    triple: 'x86_64-linux-android',
    library: 'libnobodywho_flutter.so',
  ),
};

class Config {
  Config(
    this.platform,
    this.arch,
    this.buildType,
    this.cacheDir, {
    this.component = 'main',
  });

  final String platform;
  final String? arch;
  final String buildType;
  final String cacheDir;

  /// Which artifact to resolve: the binding itself ('main') or the separately
  /// shipped ONNX Runtime .so ('onnxruntime').
  final String component;

  bool get isApple => platform == 'ios' || platform == 'macos';

  Binary get binary =>
      binaries['$platform-$arch'] ??
      (throw Exception('Unsupported platform/arch: $platform/$arch'));
}

Future<void> main(List<String> arguments) async {
  try {
    stdout.writeln(await resolve(parseArguments(arguments)));
  } catch (error) {
    stderr.writeln('Error: $error');
    exitCode = 1;
  }
}

Config parseArguments(List<String> arguments) {
  final parser = ArgParser()
    ..addOption('platform', mandatory: true)
    ..addOption('arch')
    ..addOption('build-type', mandatory: true, allowed: ['debug', 'release'])
    ..addOption('cache-dir', mandatory: true)
    ..addOption('component', defaultsTo: 'main', allowed: ['main', 'onnxruntime']);
  final values = parser.parse(arguments);
  if (values.rest.isNotEmpty) {
    throw ArgumentError('Unexpected arguments: ${values.rest.join(' ')}');
  }
  final config = Config(
    values.option('platform')!,
    values.option('arch'),
    values.option('build-type')!,
    values.option('cache-dir')!,
    component: values.option('component')!,
  );
  if (!config.isApple && config.arch == null) {
    throw ArgumentError('Missing required argument: --arch');
  }
  if (!config.isApple) config.binary;
  return config;
}

Future<String> resolve(Config config) async {
  if (config.component == 'onnxruntime') return resolveOnnxRuntime(config);
  return localBuild(config) ??
      cachedDownload(config) ??
      await downloadRelease(config);
}

String absolute(String path) => File(path).absolute.path;

Directory get targetDirectory {
  final plugin = File(Platform.script.toFilePath()).parent.parent;
  return Directory('${plugin.parent.parent.path}/target');
}

String? localBuild(Config config) {
  final directory = config.isApple
      ? Directory('${targetDirectory.path}/xcframework')
      : Directory(
          '${targetDirectory.path}/${config.binary.triple}/${config.buildType}',
        );
  if (!installed(config, directory)) return null;
  final path = installedPath(config, directory);
  stderr.writeln('Using local build: $path');
  return absolute(path);
}

Directory cacheDirectory(Config config, String version) {
  final name = config.isApple
      ? 'xcframework'
      : '${config.platform}-${config.arch}-${config.buildType}';
  return Directory('${config.cacheDir}/nobodywho/$version/$name');
}

String installedPath(Config config, Directory directory) => config.isApple
    ? '${directory.path}/nobodywho_flutter.xcframework'
    : '${directory.path}/${config.binary.library}';

bool installed(Config config, Directory directory) =>
    FileSystemEntity.typeSync(installedPath(config, directory)) !=
        FileSystemEntityType.notFound &&
    (config.isApple ||
        Directory('${directory.path}/nobodywho-runtime').existsSync());

String? cachedDownload(Config config) {
  final directory = cacheDirectory(config, version());
  if (!installed(config, directory)) return null;
  final path = installedPath(config, directory);
  stderr.writeln('Using cached binary: $path');
  return absolute(path);
}

Future<String> downloadRelease(Config config) async {
  if (config.isApple && config.buildType == 'debug') {
    throw Exception('Release XCFrameworks do not provide debug builds');
  }

  final packageVersion = version();
  final asset = config.isApple
      ? 'nobodywho_flutter.xcframework.zip'
      : 'nobodywho-flutter-${config.binary.triple}-${config.buildType}.zip';
  final url =
      'https://github.com/nobodywho-ooo/nobodywho/releases/download/nobodywho-flutter-v$packageVersion/$asset';
  final directory = cacheDirectory(config, packageVersion);
  await installArchive(url, directory, config);
  return absolute(installedPath(config, directory));
}

/// Resolves the standalone ONNX Runtime .so. Only Android x86_64 needs it —
/// everywhere else ORT is linked into the binding itself.
Future<String> resolveOnnxRuntime(Config config) async {
  final needsIt =
      onnxRuntimeArches[config.platform]?.contains(config.arch) ?? false;
  if (!needsIt) {
    throw Exception(
      'onnxruntime component was requested for ${config.platform}/${config.arch}, '
      'but it is only needed on: '
      '${onnxRuntimeArches.entries.map((e) => '${e.key}/${e.value.join(",")}').join("; ")}',
    );
  }

  // Strategy 1: cached extraction from a previous run
  final cacheBasePath =
      '${config.cacheDir}/onnxruntime/$onnxRuntimeVersion/${config.platform}-${config.arch}';
  final cachedFile = File('$cacheBasePath/libonnxruntime.so');
  if (cachedFile.existsSync()) {
    stderr.writeln('Using cached onnxruntime library: ${cachedFile.path}');
    return cachedFile.absolute.path;
  }

  // Strategy 2: download Microsoft's prebuilt AAR from Maven Central and
  // extract the .so - same artifact CI uses to link x86_64 (see build.yml).
  final url =
      'https://repo1.maven.org/maven2/com/microsoft/onnxruntime/onnxruntime-android/'
      '$onnxRuntimeVersion/onnxruntime-android-$onnxRuntimeVersion.aar';
  final aarFile = File(
    '$cacheBasePath/onnxruntime-android-$onnxRuntimeVersion.aar',
  );

  try {
    await download(url, aarFile);

    stderr.writeln('Extracting jni/${config.arch}/libonnxruntime.so...');
    final unzipResult = await Process.run('unzip', [
      '-j', '-o', '-q',
      aarFile.path,
      'jni/${config.arch}/libonnxruntime.so',
      '-d', cacheBasePath,
    ]);
    if (unzipResult.exitCode != 0) {
      throw Exception(
        'Failed to extract libonnxruntime.so from AAR: ${unzipResult.stderr}',
      );
    }
    if (!cachedFile.existsSync()) {
      throw Exception(
        'libonnxruntime.so not found in AAR after extraction: ${cachedFile.path}',
      );
    }
    stderr.writeln('Extracted to: ${cachedFile.path}');
    return cachedFile.absolute.path;
  } finally {
    if (aarFile.existsSync()) aarFile.deleteSync();
  }
}

Future<void> installArchive(
  String url,
  Directory destination,
  Config config,
) async {
  final staging = Directory('${destination.path}.tmp-$pid');
  final archive = File('${destination.path}.tmp-$pid.zip');
  stderr.writeln('Downloading: $url');
  try {
    if (staging.existsSync()) staging.deleteSync(recursive: true);
    await download(url, archive);
    await extract(archive, staging);
    if (!installed(config, staging)) {
      throw Exception('Invalid NobodyWho archive');
    }
    if (destination.existsSync()) destination.deleteSync(recursive: true);
    await staging.rename(destination.path);
  } finally {
    if (archive.existsSync()) archive.deleteSync();
    if (staging.existsSync()) staging.deleteSync(recursive: true);
  }
}

Future<void> download(String url, File output) async {
  await output.parent.create(recursive: true);
  final client = HttpClient()..connectionTimeout = httpTimeout;
  try {
    final response = await (await client.getUrl(
      Uri.parse(url),
    )).close().timeout(httpTimeout);
    if (response.statusCode != HttpStatus.ok) {
      await response.drain();
      throw Exception('Download failed: HTTP ${response.statusCode}\n$url');
    }
    await response.pipe(output.openWrite());
  } on TimeoutException {
    throw Exception('Timed out downloading $url');
  } on SocketException catch (e) {
    throw Exception('Network error downloading $url: $e');
  } finally {
    client.close(force: true);
  }
}

Future<void> extract(File archive, Directory destination) async {
  await destination.create(recursive: true);
  final result = await Process.run(
    Platform.isWindows ? 'tar' : 'unzip',
    Platform.isWindows
        ? ['-xf', archive.path, '-C', destination.path]
        : ['-q', archive.path, '-d', destination.path],
  );
  if (result.exitCode != 0)
    throw Exception('Archive extraction failed: ${result.stderr}');
}

String version() {
  final pubspec = File(
    '${File(Platform.script.toFilePath()).parent.parent.path}/pubspec.yaml',
  );
  final value = RegExp(
    r'^version:\s*(.+)$',
    multiLine: true,
  ).firstMatch(pubspec.readAsStringSync())?.group(1)?.trim();
  if (value == null) throw Exception('Could not read $pubspec');
  return value;
}
