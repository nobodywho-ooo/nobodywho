#!/usr/bin/env dart
// Binary resolution script for NobodyWho Flutter plugin
// Resolves the native library path using multiple strategies:
// 1. Environment variable override
// 2. Local cargo build
// 3. Cached download
// 4. Download from GitHub releases

import 'dart:io';
import 'dart:convert';

// Platform/architecture mappings to Rust triples and library names
const platformMappings = {
  'linux': {
    'x86_64': {
      'triple': 'x86_64-unknown-linux-gnu',
      'lib': 'libnobodywho_flutter.so',
    },
    'aarch64': {
      'triple': 'aarch64-unknown-linux-gnu',
      'lib': 'libnobodywho_flutter.so',
    },
  },
  'windows': {
    'x86_64': {
      'triple': 'x86_64-pc-windows-msvc',
      'lib': 'nobodywho_flutter.dll',
    },
  },
  'android': {
    'arm64-v8a': {
      'triple': 'aarch64-linux-android',
      'lib': 'libnobodywho_flutter.so',
    },
    // Note: 32-bit targets (armeabi-v7a, x86) are not supported due to build issues
    'x86_64': {
      'triple': 'x86_64-linux-android',
      'lib': 'libnobodywho_flutter.so',
    },
  },
};

void main(List<String> arguments) async {
  try {
    final config = parseArguments(arguments);
    final resolvedPath = await resolveBinary(config);
    stdout.writeln(resolvedPath);
    exit(0);
  } catch (e) {
    stderr.writeln('Error: $e');
    exit(1);
  }
}

class Config {
  final String platform;
  final String? arch;
  final String buildType;
  final String cacheDir;

  Config({
    required this.platform,
    this.arch,
    required this.buildType,
    required this.cacheDir,
  });

  bool get isApplePlatform => platform == 'ios' || platform == 'macos';
}

Config parseArguments(List<String> args) {
  String? platform;
  String? arch;
  String? buildType;
  String? cacheDir;

  for (int i = 0; i < args.length; i++) {
    if (args[i].startsWith('--')) {
      final parts = args[i].substring(2).split('=');
      final key = parts[0];
      final value = parts.length > 1 ? parts[1] : (i + 1 < args.length ? args[++i] : null);

      switch (key) {
        case 'platform':
          platform = value;
          break;
        case 'arch':
          arch = value;
          break;
        case 'build-type':
          buildType = value;
          break;
        case 'cache-dir':
          cacheDir = value;
          break;
      }
    }
  }

  if (platform == null) {
    throw ArgumentError('Missing required argument: --platform');
  }
  if (buildType == null) {
    throw ArgumentError('Missing required argument: --build-type');
  }
  if (cacheDir == null) {
    throw ArgumentError('Missing required argument: --cache-dir');
  }

  // Arch is not required for iOS/macOS (they use xcframework)
  if (platform != 'ios' && platform != 'macos' && arch == null) {
    throw ArgumentError('Missing required argument: --arch (required for $platform)');
  }

  if (!['debug', 'release'].contains(buildType)) {
    throw ArgumentError('Invalid build-type: $buildType (must be debug or release)');
  }

  return Config(
    platform: platform,
    arch: arch,
    buildType: buildType,
    cacheDir: cacheDir,
  );
}

Future<String> resolveBinary(Config config) async {
  // Strategy 1: Environment variable override
  final envPath = checkEnvironmentOverride(config);
  if (envPath != null) {
    return envPath;
  }

  // Strategy 2: Local cargo build
  final localPath = checkLocalBuild(config);
  if (localPath != null) {
    return localPath;
  }

  // Strategy 3: Cached download
  final cachedPath = checkCachedDownload(config);
  if (cachedPath != null) {
    return cachedPath;
  }

  // Strategy 4: Download from GitHub
  return await downloadFromGitHub(config);
}

String? checkEnvironmentOverride(Config config) {
  if (config.isApplePlatform) {
    // For iOS/macOS, check for xcframework path
    final xcframeworkPath = Platform.environment['NOBODYWHO_FLUTTER_XCFRAMEWORK_PATH'];
    if (xcframeworkPath != null && xcframeworkPath.isNotEmpty) {
      final xcframeworkDir = Directory(xcframeworkPath);
      if (xcframeworkDir.existsSync()) {
        stderr.writeln('Using xcframework from environment variable: $xcframeworkPath');
        return xcframeworkPath;
      } else {
        throw Exception(
          'NOBODYWHO_FLUTTER_XCFRAMEWORK_PATH is set but path does not exist: $xcframeworkPath'
        );
      }
    }
  } else {
    // For desktop/Android, check for library path
    final libPath = Platform.environment['NOBODYWHO_FLUTTER_LIB_PATH'];
    if (libPath != null && libPath.isNotEmpty) {
      final libFile = File(libPath);
      if (libFile.existsSync()) {
        stderr.writeln('Using library from environment variable: $libPath');
        return libPath;
      } else {
        throw Exception(
          'NOBODYWHO_FLUTTER_LIB_PATH is set but file does not exist: $libPath'
        );
      }
    }
  }
  return null;
}

String? checkLocalBuild(Config config) {
  // Find the script directory (tool/) and navigate to workspace root
  final scriptFile = File(Platform.script.toFilePath());
  final toolDir = scriptFile.parent;
  final pluginDir = toolDir.parent; // nobodywho_flutter/
  final flutterDir = pluginDir.parent; // flutter/
  final nobodywhoDir = flutterDir.parent; // nobodywho/
  final targetDir = Directory('${nobodywhoDir.path}/target');

  if (!targetDir.existsSync()) {
    return null;
  }

  if (config.isApplePlatform) {
    // For iOS/macOS, we can't easily construct xcframework from local builds
    // User should use environment variable for local development
    // Check if any .a files exist to give a helpful error
    final triples = [
      'aarch64-apple-darwin',
      'x86_64-apple-darwin',
      'aarch64-apple-ios',
      'aarch64-apple-ios-sim',
      'x86_64-apple-ios',
    ];

    for (final triple in triples) {
      final archiveFile = File('${targetDir.path}/$triple/${config.buildType}/libnobodywho_flutter.a');
      if (archiveFile.existsSync()) {
        throw Exception(
          'Found local .a files but xcframework is not built.\n'
          'For local development, set NOBODYWHO_FLUTTER_XCFRAMEWORK_PATH to point to your xcframework.\n'
          'You can build it manually from the .a files using xcodebuild -create-xcframework.'
        );
      }
    }
    return null;
  }

  // For desktop/Android, check for the library file
  final mapping = platformMappings[config.platform]?[config.arch];
  if (mapping == null) {
    return null;
  }

  final triple = mapping['triple'] as String;
  final libName = mapping['lib'] as String;
  final libFile = File('${targetDir.path}/$triple/${config.buildType}/$libName');

  if (libFile.existsSync()) {
    stderr.writeln('Using local build: ${libFile.path}');
    return libFile.absolute.path;
  }

  return null;
}

String? checkCachedDownload(Config config) {
  final version = getVersion();
  final cacheBasePath = '${config.cacheDir}/nobodywho_flutter/$version';

  if (config.isApplePlatform) {
    // Check for cached xcframework
    final xcframeworkPath = '$cacheBasePath/xcframework/NobodyWhoFlutter.xcframework';
    final xcframeworkDir = Directory(xcframeworkPath);
    if (xcframeworkDir.existsSync()) {
      stderr.writeln('Using cached xcframework: $xcframeworkPath');
      return xcframeworkPath;
    }
  } else {
    // Check for cached library file
    final mapping = platformMappings[config.platform]?[config.arch];
    if (mapping == null) {
      return null;
    }

    final libName = mapping['lib'] as String;
    final libPath = '$cacheBasePath/${config.platform}-${config.arch}/$libName';
    final libFile = File(libPath);
    if (libFile.existsSync()) {
      stderr.writeln('Using cached library: $libPath');
      return libFile.absolute.path;
    }
  }

  return null;
}

Future<String> downloadFromGitHub(Config config) async {
  final version = getVersion();

  if (config.isApplePlatform && config.buildType == 'debug') {
    throw Exception(
      'Debug builds for iOS/macOS are not provided in releases.\n'
      'For local development, set NOBODYWHO_FLUTTER_XCFRAMEWORK_PATH to point to your xcframework.\n'
      'You can build it manually from the .a files in target/{triple}/debug/ using xcodebuild.'
    );
  }

  stderr.writeln('Downloading from GitHub releases (version: $version)...');

  if (config.isApplePlatform) {
    return await downloadXCFramework(config, version);
  } else {
    return await downloadLibrary(config, version);
  }
}

Future<String> downloadLibrary(Config config, String version) async {
  final mapping = platformMappings[config.platform]?[config.arch];
  if (mapping == null) {
    throw Exception('Unsupported platform/arch: ${config.platform}/${config.arch}');
  }

  final triple = mapping['triple'] as String;
  final libName = mapping['lib'] as String;

  // Construct download URL
  // All artifacts have "lib" prefix for consistency
  final fileName = 'libnobodywho-flutter-$triple-${config.buildType}.${libName.split('.').last}';
  final url = 'https://github.com/nobodywho-ooo/nobodywho/releases/download/nobodywho-flutter-v$version/$fileName';

  // Prepare cache directory
  final cacheDir = '${config.cacheDir}/nobodywho_flutter/$version/${config.platform}-${config.arch}';
  final cacheDirObj = Directory(cacheDir);
  await cacheDirObj.create(recursive: true);

  final outputPath = '$cacheDir/$libName';
  final outputFile = File(outputPath);

  stderr.writeln('Downloading: $url');

  try {
    final httpClient = HttpClient();
    final request = await httpClient.getUrl(Uri.parse(url));
    final response = await request.close();

    if (response.statusCode != 200) {
      throw Exception(
        'Failed to download library: HTTP ${response.statusCode}\n'
        'URL: $url\n'
        'This version may not be available in releases. Check: https://github.com/nobodywho-ooo/nobodywho/releases/tag/nobodywho-flutter-v$version'
      );
    }

    final sink = outputFile.openWrite();
    await response.pipe(sink);
    await sink.close();
    httpClient.close();

    stderr.writeln('Downloaded to: $outputPath');
    return outputFile.absolute.path;
  } catch (e) {
    // Clean up partial download
    if (outputFile.existsSync()) {
      outputFile.deleteSync();
    }
    rethrow;
  }
}

Future<String> downloadXCFramework(Config config, String version) async {
  // Download URL for xcframework
  final fileName = 'NobodyWhoFlutter.xcframework.zip';
  final url = 'https://github.com/nobodywho-ooo/nobodywho/releases/download/nobodywho-flutter-v$version/$fileName';

  // Prepare cache directory
  final cacheDir = '${config.cacheDir}/nobodywho_flutter/$version/xcframework';
  final cacheDirObj = Directory(cacheDir);
  await cacheDirObj.create(recursive: true);

  final zipPath = '$cacheDir/$fileName';
  final zipFile = File(zipPath);
  final xcframeworkPath = '$cacheDir/NobodyWhoFlutter.xcframework';

  stderr.writeln('Downloading: $url');

  try {
    final httpClient = HttpClient();
    final request = await httpClient.getUrl(Uri.parse(url));
    final response = await request.close();

    if (response.statusCode != 200) {
      throw Exception(
        'Failed to download xcframework: HTTP ${response.statusCode}\n'
        'URL: $url\n'
        'This version may not be available in releases. Check: https://github.com/nobodywho-ooo/nobodywho/releases/tag/nobodywho-flutter-v$version'
      );
    }

    final sink = zipFile.openWrite();
    await response.pipe(sink);
    await sink.close();
    httpClient.close();

    stderr.writeln('Downloaded to: $zipPath');

    // Unzip the xcframework
    stderr.writeln('Extracting xcframework...');
    final unzipResult = await Process.run(
      'unzip',
      ['-o', '-q', zipPath, '-d', cacheDir],
      workingDirectory: cacheDir,
    );

    if (unzipResult.exitCode != 0) {
      throw Exception('Failed to unzip xcframework: ${unzipResult.stderr}');
    }

    // Clean up zip file
    zipFile.deleteSync();

    // Verify xcframework exists
    if (!Directory(xcframeworkPath).existsSync()) {
      throw Exception('Xcframework not found after extraction: $xcframeworkPath');
    }

    stderr.writeln('Extracted to: $xcframeworkPath');
    return xcframeworkPath;
  } catch (e) {
    // Clean up partial download
    if (zipFile.existsSync()) {
      zipFile.deleteSync();
    }
    if (Directory(xcframeworkPath).existsSync()) {
      Directory(xcframeworkPath).deleteSync(recursive: true);
    }
    rethrow;
  }
}

String getVersion() {
  // Find pubspec.yaml relative to this script
  final scriptFile = File(Platform.script.toFilePath());
  final toolDir = scriptFile.parent;
  final pluginDir = toolDir.parent;
  final pubspecFile = File('${pluginDir.path}/pubspec.yaml');

  if (!pubspecFile.existsSync()) {
    throw Exception('Could not find pubspec.yaml at: ${pubspecFile.path}');
  }

  final content = pubspecFile.readAsStringSync();

  // Parse version using regex (no yaml dependency)
  final versionRegex = RegExp(r'^version:\s*(.+)$', multiLine: true);
  final match = versionRegex.firstMatch(content);

  if (match == null || match.group(1) == null) {
    throw Exception('Could not parse version from pubspec.yaml');
  }

  final version = match.group(1)!.trim();
  return version;
}
