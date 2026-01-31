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

  // Setup for model symlinks
  buffer.writeln("    setUpAll(() async {");
  buffer.writeln("      // Create symlinks for model paths used in docs");
  buffer.writeln("      final modelPath = Platform.environment['TEST_MODEL'];");
  buffer.writeln("      final embeddingPath = Platform.environment['TEST_EMBEDDINGS_MODEL'];");
  buffer.writeln("      final rerankerPath = Platform.environment['TEST_CROSSENCODER_MODEL'];");
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
  buffer.writeln("    });");
  buffer.writeln();

  buffer.writeln("    tearDownAll(() async {");
  buffer.writeln("      // Clean up symlinks");
  buffer.writeln("      final links = ['./model.gguf', './embedding-model.gguf', './reranker-model.gguf'];");
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

    if (hasMainFunction(code)) {
      // Code has its own main - extract it as a separate function
      buffer.writeln("    test('$testDescription', () async {");
      buffer.writeln("      await _doctest_$testIndex();");
      buffer.writeln("    });");
    } else {
      // Wrap inline code in a test
      buffer.writeln("    test('$testDescription', () async {");

      // Remove imports from inline code (they're at the top)
      final codeWithoutImports = removeImports(code);

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

      // Remove imports and rename main
      var codeWithoutImports = removeImports(code);
      codeWithoutImports = codeWithoutImports
          .replaceFirst(RegExp(r'Future<void>\s+main\s*\('), 'Future<void> _doctest_$testIndex(')
          .replaceFirst(RegExp(r'void\s+main\s*\('), 'Future<void> _doctest_$testIndex(');

      buffer.writeln(codeWithoutImports);
    }
    testIndex++;
  }

  return buffer.toString();
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
      .toList();
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
