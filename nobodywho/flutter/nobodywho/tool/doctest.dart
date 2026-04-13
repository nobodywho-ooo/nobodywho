/// Dart Doctest Tool
///
/// Extracts Dart code blocks from markdown files and runs them as tests.
/// Supports continuation syntax where `{.dart continuation}` blocks are
/// appended to the previous code block.
///
/// Usage:
///   dart run tool/doctest.dart <markdown_dir> [--generate-only]
///
/// Environment variables:
///   TEST_MODEL - Path to the chat model .gguf file
///   TEST_EMBEDDINGS_MODEL - Path to the embedding model .gguf file
///   TEST_CROSSENCODER_MODEL - Path to the reranker model .gguf file

import 'dart:convert';
import 'dart:io';

/// Represents a code block extracted from markdown
class CodeBlock {
  final String code;
  final bool isContinuation;
  final int lineNumber;
  final String sourceFile;

  CodeBlock({
    required this.code,
    required this.isContinuation,
    required this.lineNumber,
    required this.sourceFile,
  });
}

/// Represents a group of code blocks that form a single test
class CodeGroup {
  final List<CodeBlock> blocks;
  final String sourceFile;
  final int startLine;

  CodeGroup({
    required this.blocks,
    required this.sourceFile,
    required this.startLine,
  });

  String get combinedCode => blocks.map((b) => b.code).join('\n');
}

/// Parses a markdown file and extracts Dart code blocks
List<CodeBlock> extractCodeBlocks(String content, String sourceFile) {
  final blocks = <CodeBlock>[];
  final lines = content.split('\n');

  var inCodeBlock = false;
  var isDartBlock = false;
  var isContinuation = false;
  var currentCode = StringBuffer();
  var blockStartLine = 0;

  for (var i = 0; i < lines.length; i++) {
    final line = lines[i];

    if (!inCodeBlock) {
      // Check for start of code block
      if (line.startsWith('```dart') || line.startsWith('```{.dart')) {
        inCodeBlock = true;
        isDartBlock = true;
        isContinuation = line.contains('continuation');
        currentCode = StringBuffer();
        blockStartLine = i + 1;
      }
    } else {
      // Check for end of code block
      if (line.startsWith('```')) {
        inCodeBlock = false;
        if (isDartBlock) {
          blocks.add(CodeBlock(
            code: currentCode.toString().trimRight(),
            isContinuation: isContinuation,
            lineNumber: blockStartLine,
            sourceFile: sourceFile,
          ));
        }
        isDartBlock = false;
      } else {
        currentCode.writeln(line);
      }
    }
  }

  return blocks;
}

/// Groups code blocks together based on continuation markers
List<CodeGroup> groupCodeBlocks(List<CodeBlock> blocks) {
  final groups = <CodeGroup>[];
  CodeGroup? currentGroup;

  for (final block in blocks) {
    if (!block.isContinuation || currentGroup == null) {
      // Start a new group
      if (currentGroup != null) {
        groups.add(currentGroup);
      }
      currentGroup = CodeGroup(
        blocks: [block],
        sourceFile: block.sourceFile,
        startLine: block.lineNumber,
      );
    } else {
      // Add to current group
      currentGroup.blocks.add(block);
    }
  }

  if (currentGroup != null) {
    groups.add(currentGroup);
  }

  return groups;
}

/// Normalize import statement (fix package names, etc.)
String normalizeImport(String importLine) {
  // Normalize nobodywho_dart to nobodywho
  importLine = importLine.replaceAll('nobodywho_dart/nobodywho_dart.dart', 'nobodywho/nobodywho.dart');
  importLine = importLine.replaceAll("'package:nobodywho_dart/", "'package:nobodywho/");
  importLine = importLine.replaceAll('"package:nobodywho_dart/', '"package:nobodywho/');
  return importLine;
}

/// Extract import statements from code
Set<String> extractImports(String code) {
  final imports = <String>{};
  // Match import statements with either single or double quotes
  final importRegex = RegExp(r'^import\s+.+;?\s*$', multiLine: true);

  for (final match in importRegex.allMatches(code)) {
    var importLine = match.group(0)!.trim();
    // Ensure it ends with semicolon
    if (!importLine.endsWith(';')) {
      importLine = '$importLine;';
    }
    // Normalize package names
    importLine = normalizeImport(importLine);
    // Normalize double quotes to single quotes to avoid duplicates
    importLine = importLine.replaceAllMapped(
      RegExp(r'^(import\s+)"(.+)"(.*);$'),
      (m) => "${m.group(1)}'${m.group(2)}'${m.group(3)};",
    );
    // Skip nobodywho imports since we add them ourselves
    if (importLine.contains('nobodywho.dart')) {
      continue;
    }
    imports.add(importLine);
  }

  return imports;
}

/// Remove import statements from code
String removeImports(String code) {
  final importRegex = RegExp(r'^import\s+.+;?\s*\n?', multiLine: true);
  return code.replaceAll(importRegex, '').trim();
}

/// Remove NobodyWho.init() calls — init is handled in setUpAll
String removeInitCalls(String code) {
  final initRegex = RegExp(r'^\s*await\s+nobodywho\.NobodyWho\.init\(\)\s*;\s*\n?', multiLine: true);
  return code.replaceAll(initRegex, '').trim();
}

/// Inject templateVariables: {"enable_thinking": false} into Chat.fromPath() calls
/// that don't already mention enable_thinking, so tests skip thinking tokens.
String injectThinkingDisable(String code) {
  return code.replaceAllMapped(
    RegExp(r'(Chat\.fromPath\()([\s\S]*?)(\n\s*\);|\);)'),
    (match) {
      final args = match.group(2)!;
      if (args.contains('enable_thinking') || args.contains('allowThinking')) {
        return match.group(0)!;
      }
      final closing = match.group(3)!;
      // Strip trailing whitespace/commas so we don't produce double commas
      final trimmedArgs = args.trimRight().replaceAll(RegExp(r',+$'), '');
      if (closing.startsWith('\n')) {
        // Multiline call: add param on its own line with matching indent
        final indent = closing.substring(1, closing.indexOf(')'));
        return '${match.group(1)}$trimmedArgs,\n$indent  templateVariables: {"enable_thinking": false}$closing';
      }
      // Single-line call: add param inline
      return '${match.group(1)}$trimmedArgs, templateVariables: {"enable_thinking": false}$closing';
    },
  );
}

/// Check if code block should be skipped
bool shouldSkipCodeBlock(String code) {
  final trimmed = code.trim();

  // Skip if it's just a class definition like SamplerPresets (API documentation)
  if (trimmed.startsWith('class ') && !trimmed.contains('void main')) {
    return true;
  }

  // Skip if it contains ... (placeholder/incomplete code)
  if (trimmed.contains('...')) {
    return true;
  }

  // Skip if it contains obvious comments indicating it's not runnable
  if (trimmed.contains('// ...')) {
    return true;
  }

  // Skip bash/shell code blocks that might have been misidentified
  if (trimmed.startsWith('flutter ') || trimmed.startsWith('dart ')) {
    return true;
  }

  return false;
}

/// Check if code has a main function
bool hasMainFunction(String code) {
  return RegExp(r'\b(Future<void>|void)\s+main\s*\(').hasMatch(code);
}

/// Generates a test file from code groups
String generateTestFile(List<CodeGroup> groups, String testName) {
  final buffer = StringBuffer();
  final allImports = <String>{};

  // First pass: collect all imports
  allImports.add("import 'dart:io';");
  allImports.add("import 'dart:typed_data';");
  allImports.add("import 'package:test/test.dart';");
  allImports.add("import 'package:nobodywho/nobodywho.dart' as nobodywho;");

  for (final group in groups) {
    final code = group.combinedCode;
    if (!shouldSkipCodeBlock(code)) {
      allImports.addAll(extractImports(code));
    }
  }

  // Write header
  buffer.writeln("// AUTO-GENERATED FILE - DO NOT EDIT");
  buffer.writeln("// Generated by tool/doctest.dart from markdown documentation");
  buffer.writeln("//");
  buffer.writeln("// ignore_for_file: unused_local_variable, unused_import");
  buffer.writeln("// ignore_for_file: avoid_print, unnecessary_string_interpolations");
  buffer.writeln();

  buffer.writeln("@Timeout(Duration(seconds: 600))");
  buffer.writeln("library;");
  buffer.writeln();

  // Write sorted imports
  final sortedImports = allImports.toList()..sort();
  for (final imp in sortedImports) {
    buffer.writeln(imp);
  }
  buffer.writeln();

  // Write main test function
  buffer.writeln("void main() {");
  buffer.writeln("  group('Doctest: $testName', () {");

  // Setup: init bridge and model symlinks
  buffer.writeln("    setUpAll(() async {");
  buffer.writeln("      // Initialize flutter_rust_bridge");
  buffer.writeln("      await nobodywho.NobodyWho.init();");
  buffer.writeln("      // Create symlinks for model paths used in docs");
  buffer.writeln("      final modelPath = Platform.environment['TEST_MODEL'];");
  buffer.writeln("      final embeddingPath = Platform.environment['TEST_EMBEDDINGS_MODEL'];");
  buffer.writeln("      final rerankerPath = Platform.environment['TEST_CROSSENCODER_MODEL'];");
  buffer.writeln("      final visionModelPath = Platform.environment['TEST_MULTIMODAL_MODEL'];");
  buffer.writeln("      final mmprojPath = Platform.environment['TEST_MULTIMODAL_MMPROJ'];");
  buffer.writeln();
  buffer.writeln("      if (modelPath != null && !File('./model.gguf').existsSync()) {");
  buffer.writeln("        Link('./model.gguf').createSync(modelPath);");
  buffer.writeln("      }");
  buffer.writeln("      if (embeddingPath != null && !File('./embedding-model.gguf').existsSync()) {");
  buffer.writeln("        Link('./embedding-model.gguf').createSync(embeddingPath);");
  buffer.writeln("      }");
  buffer.writeln("      if (rerankerPath != null && !File('./reranker-model.gguf').existsSync()) {");
  buffer.writeln("        Link('./reranker-model.gguf').createSync(rerankerPath);");
  buffer.writeln("      }");
  buffer.writeln("      if (visionModelPath != null && !File('./vision-model.gguf').existsSync()) {");
  buffer.writeln("        Link('./vision-model.gguf').createSync(visionModelPath);");
  buffer.writeln("      }");
  buffer.writeln("      if (mmprojPath != null && !File('./mmproj.gguf').existsSync()) {");
  buffer.writeln("        Link('./mmproj.gguf').createSync(mmprojPath);");
  buffer.writeln("      }");
  buffer.writeln("      // Create symlinks for test images used in vision docs");
  buffer.writeln("      final testDir = '\${Directory.current.path}/test';");
  buffer.writeln("      for (final image in ['dog.png', 'penguin.png']) {");
  buffer.writeln("        if (!File('./\$image').existsSync() && File('\$testDir/\$image').existsSync()) {");
  buffer.writeln("          Link('./\$image').createSync('\$testDir/\$image');");
  buffer.writeln("        }");
  buffer.writeln("      }");
  buffer.writeln("    });");
  buffer.writeln();

  buffer.writeln("    tearDownAll(() async {");
  buffer.writeln("      // Clean up symlinks");
  buffer.writeln("      final links = ['./model.gguf', './embedding-model.gguf', './reranker-model.gguf', './vision-model.gguf', './mmproj.gguf', './dog.png', './penguin.png'];");
  buffer.writeln("      for (final path in links) {");
  buffer.writeln("        final link = Link(path);");
  buffer.writeln("        if (link.existsSync()) {");
  buffer.writeln("          link.deleteSync();");
  buffer.writeln("        }");
  buffer.writeln("      }");
  buffer.writeln("    });");
  buffer.writeln();

  // Generate a test for each code group
  var testIndex = 0;
  for (final group in groups) {
    final code = group.combinedCode;

    // Skip blocks that shouldn't be tested
    if (shouldSkipCodeBlock(code)) {
      continue;
    }

    final testDescription = '${group.sourceFile}:${group.startLine}';

    // Add skip guard for vision tests that require multimodal models
    final needsVisionModel = code.contains('vision-model.gguf') || code.contains('mmproj.gguf');

    if (hasMainFunction(code)) {
      // Code has its own main - extract it as a separate function
      buffer.writeln("    test('$testDescription', () async {");
      if (needsVisionModel) {
        buffer.writeln("      if (Platform.environment['TEST_MULTIMODAL_MODEL'] == null || Platform.environment['TEST_MULTIMODAL_MMPROJ'] == null) return;");
      }
      buffer.writeln("      await _doctest_$testIndex();");
      buffer.writeln("    });");
    } else {
      // Wrap inline code in a test
      buffer.writeln("    test('$testDescription', () async {");
      if (needsVisionModel) {
        buffer.writeln("      if (Platform.environment['TEST_MULTIMODAL_MODEL'] == null || Platform.environment['TEST_MULTIMODAL_MMPROJ'] == null) return;");
      }

      // Remove imports and init calls from inline code (they're handled in setup)
      var codeWithoutImports = removeImports(code);
      codeWithoutImports = removeInitCalls(codeWithoutImports);

      // Indent the code
      final indentedCode = codeWithoutImports.split('\n').map((l) => '      $l').join('\n');
      buffer.writeln(indentedCode);

      buffer.writeln("    });");
    }
    buffer.writeln();
    testIndex++;
  }

  buffer.writeln("  });");
  buffer.writeln("}");

  // Add extracted main functions
  testIndex = 0;
  for (final group in groups) {
    final code = group.combinedCode;

    if (shouldSkipCodeBlock(code)) {
      continue;
    }

    if (hasMainFunction(code)) {
      buffer.writeln();
      buffer.writeln("// Extracted from ${group.sourceFile}:${group.startLine}");

      // Remove imports, init calls, and rename main
      var codeWithoutImports = removeImports(code);
      codeWithoutImports = removeInitCalls(codeWithoutImports);
      codeWithoutImports = codeWithoutImports
          .replaceFirst(RegExp(r'Future<void>\s+main\s*\('), 'Future<void> _doctest_$testIndex(')
          .replaceFirst(RegExp(r'void\s+main\s*\('), 'Future<void> _doctest_$testIndex(');

      buffer.writeln(codeWithoutImports);
    }
    testIndex++;
  }

  return injectThinkingDisable(buffer.toString());
}

/// Finds all markdown files in a directory
List<File> findMarkdownFiles(String dirPath) {
  final dir = Directory(dirPath);
  if (!dir.existsSync()) {
    print('Error: Directory not found: $dirPath');
    exit(1);
  }

  return dir
      .listSync(recursive: true)
      .whereType<File>()
      .where((f) => f.path.endsWith('.md'))
      .toList()
    ..sort((a, b) => a.path.compareTo(b.path));
}

void main(List<String> args) async {
  // Resolve paths relative to the script location
  final scriptDir = File(Platform.script.toFilePath()).parent;
  final packageDir = scriptDir.parent; // flutter/nobodywho
  final outputPath = '${packageDir.path}/test/doctest_generated_test.dart';

  if (args.isEmpty) {
    print('Usage: dart run tool/doctest.dart <markdown_dir> [--generate-only]');
    print('');
    print('Options:');
    print('  --generate-only  Only generate the test file, do not run it');
    print('');
    print('Output: $outputPath');
    exit(1);
  }

  final markdownDir = args[0];
  final generateOnly = args.contains('--generate-only');

  print('Scanning for markdown files in: $markdownDir');

  final markdownFiles = findMarkdownFiles(markdownDir);
  print('Found ${markdownFiles.length} markdown files');

  final allGroups = <CodeGroup>[];

  for (final file in markdownFiles) {
    final content = file.readAsStringSync();
    final relativePath = file.path.replaceFirst(markdownDir, '').replaceFirst(RegExp(r'^[/\\]'), '');

    final blocks = extractCodeBlocks(content, relativePath);
    if (blocks.isNotEmpty) {
      print('  $relativePath: ${blocks.length} code blocks');
      final groups = groupCodeBlocks(blocks);
      allGroups.addAll(groups);
    }
  }

  print('');
  print('Total code groups: ${allGroups.length}');

  // Count skipped blocks
  var skippedCount = 0;
  for (final group in allGroups) {
    if (shouldSkipCodeBlock(group.combinedCode)) {
      skippedCount++;
    }
  }
  print('Skipped (incomplete/non-runnable): $skippedCount');
  print('Tests to generate: ${allGroups.length - skippedCount}');

  if (allGroups.isEmpty) {
    print('No Dart code blocks found in markdown files.');
    exit(0);
  }

  // Generate test file
  final testContent = generateTestFile(allGroups, 'Flutter Docs');
  final testFile = File(outputPath);
  testFile.writeAsStringSync(testContent);
  print('');
  print('Generated test file: ${testFile.path}');

  if (generateOnly) {
    print('');
    print('Run tests with: flutter test test/doctest_generated_test.dart');
    exit(0);
  }

  // Run tests
  print('');
  print('Running tests...');
  print('');

  final process = await Process.start(
    'flutter',
    ['test', 'test/doctest_generated_test.dart'],
    workingDirectory: packageDir.path,
  );

  // Stream output in real-time
  process.stdout.transform(const SystemEncoding().decoder).listen(stdout.write);
  process.stderr.transform(const SystemEncoding().decoder).listen(stderr.write);

  final exitCode = await process.exitCode;
  exit(exitCode);
}
