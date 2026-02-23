library;

import 'dart:async';
import 'dart:convert';

export 'src/rust/lib.dart'
    hide
        RustChat, // Users should use Chat
        RustTokenStream, // Users should use TokenStream
        RustTool, // Users should use Tool
        newToolImpl, // Internal helper
        toolCallArgumentsJson, // Internal helper
        PromptPart; // Users should use the hand-written PromptPart sealed class
export 'src/rust/frb_generated.dart' show NobodyWho;

import 'src/rust/lib.dart' as nobodywho;

/// A part of a multimodal prompt.
sealed class PromptPart {}

/// A text part of a prompt.
final class TextPart extends PromptPart {
  final String text;
  const TextPart(this.text);
}

/// An image part of a prompt, identified by file path.
final class ImagePart extends PromptPart {
  final String path;
  const ImagePart(this.path);
}

/// A multimodal prompt consisting of one or more [PromptPart]s (text and/or images).
///
/// Example:
/// ```dart
/// final prompt = Prompt([TextPart("Describe this image:"), ImagePart("/path/to/img.jpg")]);
/// final stream = chat.ask(prompt);
/// ```
class Prompt {
  final List<PromptPart> parts;

  const Prompt(this.parts);

  /// Convenience factory for text-only prompts.
  factory Prompt.text(String text) => Prompt([TextPart(text)]);
}

List<nobodywho.PromptPart> _convertPromptParts(List<PromptPart> parts) {
  return parts.map((p) => switch (p) {
    TextPart(:final text) => nobodywho.PromptPart.text(content: text),
    ImagePart(:final path) => nobodywho.PromptPart.image(path: path),
  }).toList();
}

/// Converts JSON-decoded data to properly typed Dart values based on a JSON schema.
/// Handles primitives, nested Lists (up to 3 levels), Sets, and Maps.
/// Uses explicit casts at every level to ensure proper Dart types.
dynamic jsonConversion(Map<String, dynamic> schema, dynamic json) {
  final t1 = schema["type"] as String;
  final u1 = schema["uniqueItems"] == true || schema["uniqueItems"] == "true";

  // Primitives
  if (t1 == "integer") return (json as num).toInt();
  if (t1 == "number") return (json as num).toDouble();
  if (t1 == "string") return json as String;
  if (t1 == "boolean") return json as bool;

  // Arrays (List or Set)
  if (t1 == "array") {
    final s2 = schema["items"] as Map<String, dynamic>;
    final t2 = s2["type"] as String;
    final u2 = s2["uniqueItems"] == true || s2["uniqueItems"] == "true";

    // List/Set of primitives
    if (t2 == "integer") {
      return u1 ? (json as List).cast<int>().toSet() : (json as List).cast<int>().toList();
    }
    if (t2 == "number") {
      return u1
          ? (json as List).map((e) => (e as num).toDouble()).toSet()
          : (json as List).map((e) => (e as num).toDouble()).toList();
    }
    if (t2 == "string") {
      return u1 ? (json as List).cast<String>().toSet() : (json as List).cast<String>().toList();
    }
    if (t2 == "boolean") {
      return u1 ? (json as List).cast<bool>().toSet() : (json as List).cast<bool>().toList();
    }

    // List/Set of Arrays (level 2)
    if (t2 == "array") {
      final s3 = s2["items"] as Map<String, dynamic>;
      final t3 = s3["type"] as String;
      final u3 = s3["uniqueItems"] == true || s3["uniqueItems"] == "true";

      // List/Set of List/Set of primitives
      if (t3 == "integer") {
        if (u1 && u2) return (json as List).map((e) => (e as List).cast<int>().toSet()).toSet();
        if (u1 && !u2) return (json as List).map((e) => (e as List).cast<int>().toList()).toSet();
        if (!u1 && u2) return (json as List).map((e) => (e as List).cast<int>().toSet()).toList();
        return (json as List).map((e) => (e as List).cast<int>().toList()).toList();
      }
      if (t3 == "number") {
        if (u1 && u2) return (json as List).map((e) => (e as List).map((x) => (x as num).toDouble()).toSet()).toSet();
        if (u1 && !u2) return (json as List).map((e) => (e as List).map((x) => (x as num).toDouble()).toList()).toSet();
        if (!u1 && u2) return (json as List).map((e) => (e as List).map((x) => (x as num).toDouble()).toSet()).toList();
        return (json as List).map((e) => (e as List).map((x) => (x as num).toDouble()).toList()).toList();
      }
      if (t3 == "string") {
        if (u1 && u2) return (json as List).map((e) => (e as List).cast<String>().toSet()).toSet();
        if (u1 && !u2) return (json as List).map((e) => (e as List).cast<String>().toList()).toSet();
        if (!u1 && u2) return (json as List).map((e) => (e as List).cast<String>().toSet()).toList();
        return (json as List).map((e) => (e as List).cast<String>().toList()).toList();
      }
      if (t3 == "boolean") {
        if (u1 && u2) return (json as List).map((e) => (e as List).cast<bool>().toSet()).toSet();
        if (u1 && !u2) return (json as List).map((e) => (e as List).cast<bool>().toList()).toSet();
        if (!u1 && u2) return (json as List).map((e) => (e as List).cast<bool>().toSet()).toList();
        return (json as List).map((e) => (e as List).cast<bool>().toList()).toList();
      }

      // List/Set of List/Set of Arrays (level 3)
      if (t3 == "array") {
        final s4 = s3["items"] as Map<String, dynamic>;
        final t4 = s4["type"] as String;

        if (t4 == "integer") {
          if (!u1 && !u2 && !u3) return (json as List).map((e1) => (e1 as List).map((e2) => (e2 as List).cast<int>().toList()).toList()).toList();
          if (!u1 && !u2 && u3) return (json as List).map((e1) => (e1 as List).map((e2) => (e2 as List).cast<int>().toSet()).toList()).toList();
          if (!u1 && u2 && !u3) return (json as List).map((e1) => (e1 as List).map((e2) => (e2 as List).cast<int>().toList()).toSet()).toList();
          if (!u1 && u2 && u3) return (json as List).map((e1) => (e1 as List).map((e2) => (e2 as List).cast<int>().toSet()).toSet()).toList();
          if (u1 && !u2 && !u3) return (json as List).map((e1) => (e1 as List).map((e2) => (e2 as List).cast<int>().toList()).toList()).toSet();
          if (u1 && !u2 && u3) return (json as List).map((e1) => (e1 as List).map((e2) => (e2 as List).cast<int>().toSet()).toList()).toSet();
          if (u1 && u2 && !u3) return (json as List).map((e1) => (e1 as List).map((e2) => (e2 as List).cast<int>().toList()).toSet()).toSet();
          if (u1 && u2 && u3) return (json as List).map((e1) => (e1 as List).map((e2) => (e2 as List).cast<int>().toSet()).toSet()).toSet();
        }
        if (t4 == "number") {
          if (!u1 && !u2 && !u3) return (json as List).map((e1) => (e1 as List).map((e2) => (e2 as List).map((x) => (x as num).toDouble()).toList()).toList()).toList();
          if (!u1 && !u2 && u3) return (json as List).map((e1) => (e1 as List).map((e2) => (e2 as List).map((x) => (x as num).toDouble()).toSet()).toList()).toList();
          if (!u1 && u2 && !u3) return (json as List).map((e1) => (e1 as List).map((e2) => (e2 as List).map((x) => (x as num).toDouble()).toList()).toSet()).toList();
          if (!u1 && u2 && u3) return (json as List).map((e1) => (e1 as List).map((e2) => (e2 as List).map((x) => (x as num).toDouble()).toSet()).toSet()).toList();
          if (u1 && !u2 && !u3) return (json as List).map((e1) => (e1 as List).map((e2) => (e2 as List).map((x) => (x as num).toDouble()).toList()).toList()).toSet();
          if (u1 && !u2 && u3) return (json as List).map((e1) => (e1 as List).map((e2) => (e2 as List).map((x) => (x as num).toDouble()).toSet()).toList()).toSet();
          if (u1 && u2 && !u3) return (json as List).map((e1) => (e1 as List).map((e2) => (e2 as List).map((x) => (x as num).toDouble()).toList()).toSet()).toSet();
          if (u1 && u2 && u3) return (json as List).map((e1) => (e1 as List).map((e2) => (e2 as List).map((x) => (x as num).toDouble()).toSet()).toSet()).toSet();
        }
        if (t4 == "string") {
          if (!u1 && !u2 && !u3) return (json as List).map((e1) => (e1 as List).map((e2) => (e2 as List).cast<String>().toList()).toList()).toList();
          if (!u1 && !u2 && u3) return (json as List).map((e1) => (e1 as List).map((e2) => (e2 as List).cast<String>().toSet()).toList()).toList();
          if (!u1 && u2 && !u3) return (json as List).map((e1) => (e1 as List).map((e2) => (e2 as List).cast<String>().toList()).toSet()).toList();
          if (!u1 && u2 && u3) return (json as List).map((e1) => (e1 as List).map((e2) => (e2 as List).cast<String>().toSet()).toSet()).toList();
          if (u1 && !u2 && !u3) return (json as List).map((e1) => (e1 as List).map((e2) => (e2 as List).cast<String>().toList()).toList()).toSet();
          if (u1 && !u2 && u3) return (json as List).map((e1) => (e1 as List).map((e2) => (e2 as List).cast<String>().toSet()).toList()).toSet();
          if (u1 && u2 && !u3) return (json as List).map((e1) => (e1 as List).map((e2) => (e2 as List).cast<String>().toList()).toSet()).toSet();
          if (u1 && u2 && u3) return (json as List).map((e1) => (e1 as List).map((e2) => (e2 as List).cast<String>().toSet()).toSet()).toSet();
        }
        if (t4 == "boolean") {
          if (!u1 && !u2 && !u3) return (json as List).map((e1) => (e1 as List).map((e2) => (e2 as List).cast<bool>().toList()).toList()).toList();
          if (!u1 && !u2 && u3) return (json as List).map((e1) => (e1 as List).map((e2) => (e2 as List).cast<bool>().toSet()).toList()).toList();
          if (!u1 && u2 && !u3) return (json as List).map((e1) => (e1 as List).map((e2) => (e2 as List).cast<bool>().toList()).toSet()).toList();
          if (!u1 && u2 && u3) return (json as List).map((e1) => (e1 as List).map((e2) => (e2 as List).cast<bool>().toSet()).toSet()).toList();
          if (u1 && !u2 && !u3) return (json as List).map((e1) => (e1 as List).map((e2) => (e2 as List).cast<bool>().toList()).toList()).toSet();
          if (u1 && !u2 && u3) return (json as List).map((e1) => (e1 as List).map((e2) => (e2 as List).cast<bool>().toSet()).toList()).toSet();
          if (u1 && u2 && !u3) return (json as List).map((e1) => (e1 as List).map((e2) => (e2 as List).cast<bool>().toList()).toSet()).toSet();
          if (u1 && u2 && u3) return (json as List).map((e1) => (e1 as List).map((e2) => (e2 as List).cast<bool>().toSet()).toSet()).toSet();
        }
      }

      // List/Set of List/Set of Map (level 3)
      if (t3 == "object") {
        final a3 = s3["additionalProperties"] as Map<String, dynamic>;
        final t4 = a3["type"] as String;

        if (t4 == "integer") {
          if (!u1 && !u2) return (json as List).map((e1) => (e1 as List).map((e2) => (e2 as Map).cast<String, int>()).toList()).toList();
          if (!u1 && u2) return (json as List).map((e1) => (e1 as List).map((e2) => (e2 as Map).cast<String, int>()).toSet()).toList();
          if (u1 && !u2) return (json as List).map((e1) => (e1 as List).map((e2) => (e2 as Map).cast<String, int>()).toList()).toSet();
          if (u1 && u2) return (json as List).map((e1) => (e1 as List).map((e2) => (e2 as Map).cast<String, int>()).toSet()).toSet();
        }
        if (t4 == "number") {
          if (!u1 && !u2) return (json as List).map((e1) => (e1 as List).map((e2) => (e2 as Map).map((k, v) => MapEntry(k as String, (v as num).toDouble()))).toList()).toList();
          if (!u1 && u2) return (json as List).map((e1) => (e1 as List).map((e2) => (e2 as Map).map((k, v) => MapEntry(k as String, (v as num).toDouble()))).toSet()).toList();
          if (u1 && !u2) return (json as List).map((e1) => (e1 as List).map((e2) => (e2 as Map).map((k, v) => MapEntry(k as String, (v as num).toDouble()))).toList()).toSet();
          if (u1 && u2) return (json as List).map((e1) => (e1 as List).map((e2) => (e2 as Map).map((k, v) => MapEntry(k as String, (v as num).toDouble()))).toSet()).toSet();
        }
        if (t4 == "string") {
          if (!u1 && !u2) return (json as List).map((e1) => (e1 as List).map((e2) => (e2 as Map).cast<String, String>()).toList()).toList();
          if (!u1 && u2) return (json as List).map((e1) => (e1 as List).map((e2) => (e2 as Map).cast<String, String>()).toSet()).toList();
          if (u1 && !u2) return (json as List).map((e1) => (e1 as List).map((e2) => (e2 as Map).cast<String, String>()).toList()).toSet();
          if (u1 && u2) return (json as List).map((e1) => (e1 as List).map((e2) => (e2 as Map).cast<String, String>()).toSet()).toSet();
        }
        if (t4 == "boolean") {
          if (!u1 && !u2) return (json as List).map((e1) => (e1 as List).map((e2) => (e2 as Map).cast<String, bool>()).toList()).toList();
          if (!u1 && u2) return (json as List).map((e1) => (e1 as List).map((e2) => (e2 as Map).cast<String, bool>()).toSet()).toList();
          if (u1 && !u2) return (json as List).map((e1) => (e1 as List).map((e2) => (e2 as Map).cast<String, bool>()).toList()).toSet();
          if (u1 && u2) return (json as List).map((e1) => (e1 as List).map((e2) => (e2 as Map).cast<String, bool>()).toSet()).toSet();
        }
      }
    }

    // List/Set of Map (level 2)
    if (t2 == "object") {
      final a2 = s2["additionalProperties"] as Map<String, dynamic>;
      final t3 = a2["type"] as String;

      // List/Set of Map<String, primitive>
      if (t3 == "integer") {
        return u1
            ? (json as List).map((e) => (e as Map).cast<String, int>()).toSet()
            : (json as List).map((e) => (e as Map).cast<String, int>()).toList();
      }
      if (t3 == "number") {
        return u1
            ? (json as List).map((e) => (e as Map).map((k, v) => MapEntry(k as String, (v as num).toDouble()))).toSet()
            : (json as List).map((e) => (e as Map).map((k, v) => MapEntry(k as String, (v as num).toDouble()))).toList();
      }
      if (t3 == "string") {
        return u1
            ? (json as List).map((e) => (e as Map).cast<String, String>()).toSet()
            : (json as List).map((e) => (e as Map).cast<String, String>()).toList();
      }
      if (t3 == "boolean") {
        return u1
            ? (json as List).map((e) => (e as Map).cast<String, bool>()).toSet()
            : (json as List).map((e) => (e as Map).cast<String, bool>()).toList();
      }

      // List/Set of Map<String, List/Set> (level 3)
      if (t3 == "array") {
        final s4 = a2["items"] as Map<String, dynamic>;
        final t4 = s4["type"] as String;
        final u4 = s4["uniqueItems"] == true || s4["uniqueItems"] == "true";

        if (t4 == "integer") {
          if (!u1 && !u4) return (json as List).map((e1) => (e1 as Map).map((k, v) => MapEntry(k as String, (v as List).cast<int>().toList()))).toList();
          if (!u1 && u4) return (json as List).map((e1) => (e1 as Map).map((k, v) => MapEntry(k as String, (v as List).cast<int>().toSet()))).toList();
          if (u1 && !u4) return (json as List).map((e1) => (e1 as Map).map((k, v) => MapEntry(k as String, (v as List).cast<int>().toList()))).toSet();
          if (u1 && u4) return (json as List).map((e1) => (e1 as Map).map((k, v) => MapEntry(k as String, (v as List).cast<int>().toSet()))).toSet();
        }
        if (t4 == "number") {
          if (!u1 && !u4) return (json as List).map((e1) => (e1 as Map).map((k, v) => MapEntry(k as String, (v as List).map((x) => (x as num).toDouble()).toList()))).toList();
          if (!u1 && u4) return (json as List).map((e1) => (e1 as Map).map((k, v) => MapEntry(k as String, (v as List).map((x) => (x as num).toDouble()).toSet()))).toList();
          if (u1 && !u4) return (json as List).map((e1) => (e1 as Map).map((k, v) => MapEntry(k as String, (v as List).map((x) => (x as num).toDouble()).toList()))).toSet();
          if (u1 && u4) return (json as List).map((e1) => (e1 as Map).map((k, v) => MapEntry(k as String, (v as List).map((x) => (x as num).toDouble()).toSet()))).toSet();
        }
        if (t4 == "string") {
          if (!u1 && !u4) return (json as List).map((e1) => (e1 as Map).map((k, v) => MapEntry(k as String, (v as List).cast<String>().toList()))).toList();
          if (!u1 && u4) return (json as List).map((e1) => (e1 as Map).map((k, v) => MapEntry(k as String, (v as List).cast<String>().toSet()))).toList();
          if (u1 && !u4) return (json as List).map((e1) => (e1 as Map).map((k, v) => MapEntry(k as String, (v as List).cast<String>().toList()))).toSet();
          if (u1 && u4) return (json as List).map((e1) => (e1 as Map).map((k, v) => MapEntry(k as String, (v as List).cast<String>().toSet()))).toSet();
        }
        if (t4 == "boolean") {
          if (!u1 && !u4) return (json as List).map((e1) => (e1 as Map).map((k, v) => MapEntry(k as String, (v as List).cast<bool>().toList()))).toList();
          if (!u1 && u4) return (json as List).map((e1) => (e1 as Map).map((k, v) => MapEntry(k as String, (v as List).cast<bool>().toSet()))).toList();
          if (u1 && !u4) return (json as List).map((e1) => (e1 as Map).map((k, v) => MapEntry(k as String, (v as List).cast<bool>().toList()))).toSet();
          if (u1 && u4) return (json as List).map((e1) => (e1 as Map).map((k, v) => MapEntry(k as String, (v as List).cast<bool>().toSet()))).toSet();
        }
      }
    }
  }

  // Objects (Map)
  if (t1 == "object") {
    final a1 = schema["additionalProperties"] as Map<String, dynamic>;
    final t2 = a1["type"] as String;

    // Map<String, primitive>
    if (t2 == "integer") return (json as Map).cast<String, int>();
    if (t2 == "number") return (json as Map).map((k, v) => MapEntry(k as String, (v as num).toDouble()));
    if (t2 == "string") return (json as Map).cast<String, String>();
    if (t2 == "boolean") return (json as Map).cast<String, bool>();

    // Map<String, List/Set>
    if (t2 == "array") {
      final s3 = a1["items"] as Map<String, dynamic>;
      final t3 = s3["type"] as String;
      final u3 = s3["uniqueItems"] == true || s3["uniqueItems"] == "true";

      if (t3 == "integer") {
        return u3
            ? (json as Map).map((k, v) => MapEntry(k as String, (v as List).cast<int>().toSet()))
            : (json as Map).map((k, v) => MapEntry(k as String, (v as List).cast<int>().toList()));
      }
      if (t3 == "number") {
        return u3
            ? (json as Map).map((k, v) => MapEntry(k as String, (v as List).map((x) => (x as num).toDouble()).toSet()))
            : (json as Map).map((k, v) => MapEntry(k as String, (v as List).map((x) => (x as num).toDouble()).toList()));
      }
      if (t3 == "string") {
        return u3
            ? (json as Map).map((k, v) => MapEntry(k as String, (v as List).cast<String>().toSet()))
            : (json as Map).map((k, v) => MapEntry(k as String, (v as List).cast<String>().toList()));
      }
      if (t3 == "boolean") {
        return u3
            ? (json as Map).map((k, v) => MapEntry(k as String, (v as List).cast<bool>().toSet()))
            : (json as Map).map((k, v) => MapEntry(k as String, (v as List).cast<bool>().toList()));
      }

      // Map<String, List/Set of List/Set> (level 3)
      if (t3 == "array") {
        final s4 = s3["items"] as Map<String, dynamic>;
        final t4 = s4["type"] as String;
        final u4 = s4["uniqueItems"] == true || s4["uniqueItems"] == "true";

        if (t4 == "integer") {
          if (!u3 && !u4) return (json as Map).map((k, v) => MapEntry(k as String, (v as List).map((e) => (e as List).cast<int>().toList()).toList()));
          if (!u3 && u4) return (json as Map).map((k, v) => MapEntry(k as String, (v as List).map((e) => (e as List).cast<int>().toSet()).toList()));
          if (u3 && !u4) return (json as Map).map((k, v) => MapEntry(k as String, (v as List).map((e) => (e as List).cast<int>().toList()).toSet()));
          if (u3 && u4) return (json as Map).map((k, v) => MapEntry(k as String, (v as List).map((e) => (e as List).cast<int>().toSet()).toSet()));
        }
        if (t4 == "number") {
          if (!u3 && !u4) return (json as Map).map((k, v) => MapEntry(k as String, (v as List).map((e) => (e as List).map((x) => (x as num).toDouble()).toList()).toList()));
          if (!u3 && u4) return (json as Map).map((k, v) => MapEntry(k as String, (v as List).map((e) => (e as List).map((x) => (x as num).toDouble()).toSet()).toList()));
          if (u3 && !u4) return (json as Map).map((k, v) => MapEntry(k as String, (v as List).map((e) => (e as List).map((x) => (x as num).toDouble()).toList()).toSet()));
          if (u3 && u4) return (json as Map).map((k, v) => MapEntry(k as String, (v as List).map((e) => (e as List).map((x) => (x as num).toDouble()).toSet()).toSet()));
        }
        if (t4 == "string") {
          if (!u3 && !u4) return (json as Map).map((k, v) => MapEntry(k as String, (v as List).map((e) => (e as List).cast<String>().toList()).toList()));
          if (!u3 && u4) return (json as Map).map((k, v) => MapEntry(k as String, (v as List).map((e) => (e as List).cast<String>().toSet()).toList()));
          if (u3 && !u4) return (json as Map).map((k, v) => MapEntry(k as String, (v as List).map((e) => (e as List).cast<String>().toList()).toSet()));
          if (u3 && u4) return (json as Map).map((k, v) => MapEntry(k as String, (v as List).map((e) => (e as List).cast<String>().toSet()).toSet()));
        }
        if (t4 == "boolean") {
          if (!u3 && !u4) return (json as Map).map((k, v) => MapEntry(k as String, (v as List).map((e) => (e as List).cast<bool>().toList()).toList()));
          if (!u3 && u4) return (json as Map).map((k, v) => MapEntry(k as String, (v as List).map((e) => (e as List).cast<bool>().toSet()).toList()));
          if (u3 && !u4) return (json as Map).map((k, v) => MapEntry(k as String, (v as List).map((e) => (e as List).cast<bool>().toList()).toSet()));
          if (u3 && u4) return (json as Map).map((k, v) => MapEntry(k as String, (v as List).map((e) => (e as List).cast<bool>().toSet()).toSet()));
        }
      }

      // Map<String, List/Set of Map> (level 3)
      if (t3 == "object") {
        final a4 = s3["additionalProperties"] as Map<String, dynamic>;
        final t4 = a4["type"] as String;

        if (t4 == "integer") {
          return u3
              ? (json as Map).map((k, v) => MapEntry(k as String, (v as List).map((e) => (e as Map).cast<String, int>()).toSet()))
              : (json as Map).map((k, v) => MapEntry(k as String, (v as List).map((e) => (e as Map).cast<String, int>()).toList()));
        }
        if (t4 == "number") {
          return u3
              ? (json as Map).map((k, v) => MapEntry(k as String, (v as List).map((e) => (e as Map).map((k2, v2) => MapEntry(k2 as String, (v2 as num).toDouble()))).toSet()))
              : (json as Map).map((k, v) => MapEntry(k as String, (v as List).map((e) => (e as Map).map((k2, v2) => MapEntry(k2 as String, (v2 as num).toDouble()))).toList()));
        }
        if (t4 == "string") {
          return u3
              ? (json as Map).map((k, v) => MapEntry(k as String, (v as List).map((e) => (e as Map).cast<String, String>()).toSet()))
              : (json as Map).map((k, v) => MapEntry(k as String, (v as List).map((e) => (e as Map).cast<String, String>()).toList()));
        }
        if (t4 == "boolean") {
          return u3
              ? (json as Map).map((k, v) => MapEntry(k as String, (v as List).map((e) => (e as Map).cast<String, bool>()).toSet()))
              : (json as Map).map((k, v) => MapEntry(k as String, (v as List).map((e) => (e as Map).cast<String, bool>()).toList()));
        }
      }
    }

    // Map<String, Map>
    if (t2 == "object") {
      final a3 = a1["additionalProperties"] as Map<String, dynamic>;
      final t3 = a3["type"] as String;

      if (t3 == "integer") return (json as Map).map((k, v) => MapEntry(k as String, (v as Map).cast<String, int>()));
      if (t3 == "number") return (json as Map).map((k, v) => MapEntry(k as String, (v as Map).map((k2, v2) => MapEntry(k2 as String, (v2 as num).toDouble()))));
      if (t3 == "string") return (json as Map).map((k, v) => MapEntry(k as String, (v as Map).cast<String, String>()));
      if (t3 == "boolean") return (json as Map).map((k, v) => MapEntry(k as String, (v as Map).cast<String, bool>()));
    }
  }

  // Fallback
  return json;
}




/// Extension to provide convenient access to ToolCall arguments.
/// The underlying `arguments` field is an opaque serde_json::Value,
/// so we provide these helper methods to access it as JSON string or Map.
extension ToolCallExtension on nobodywho.ToolCall {
  /// Get the arguments as a JSON string
  String get argumentsJson => nobodywho.toolCallArgumentsJson(toolCall: this);

  /// Get the arguments as a parsed Map
  Map<String, dynamic> get argumentsMap =>
      json.decode(nobodywho.toolCallArgumentsJson(toolCall: this))
          as Map<String, dynamic>;
}

// Wrapper for the RustTool class. We wrap RustTool so the API for constructing a tool
// is simply passing the arguments to a constructor.
class Tool {
  final nobodywho.RustTool _tool;

  /// Private constructor
  Tool._(this._tool);

  /// Create a tool from a Dart function.
  factory Tool({
    required Function function,
    required String name,
    required String description,
    Map<String, String> parameterDescriptions = const <String, String>{},
  }) {
    // Wrapper needs to be written in Dart to access `function.runtimeType`
    // and to deal with dynamic function parameters


    // Schema will be populated after newToolImpl returns.
    // NOTE: Dart closures capture variables by reference, not by value.
    // When wrappedFunction is defined below, schema is null. But wrappedFunction
    // isn't called until later (when the LLM invokes the tool). By that time,
    // the assignment after newToolImpl has already happened, so the closure
    // sees the populated schema value.
    Map<String, dynamic>? schema;


    // Make it a String -> Future<String> function
    final wrappedFunction = (String jsonString) async {
      // Decode the input string as json
      Map<String, dynamic> jsonMap = json.decode(jsonString);
      // Make it a map of symbols, to make Function.apply happy

      Map<Symbol, dynamic> namedParams = Map.fromEntries(
        jsonMap.entries.map((e) => MapEntry(Symbol(e.key), jsonConversion(schema!["properties"][e.key], e.value))),
      );

      // Call the function and catch any errors
      try {
        final result = Function.apply(function, [], namedParams);

        // Handle async tools and return
        if (result is Future) {
          return (await result).toString();
        } else {
          return result.toString();
        }
      } catch (e) {
        return "Error: $e";
      }
    };

    final tool = nobodywho.newToolImpl(
      function: wrappedFunction,
      name: name,
      description: description,
      runtimeType: function.runtimeType.toString(),
      parameterDescriptions : parameterDescriptions,
    );

    // Get the schema from the tool for runtime type conversion.
    // This assignment happens before wrappedFunction is ever called.
    schema = json.decode(tool.getSchemaJson()) as Map<String, dynamic>;


    return Tool._(tool);
  }

  /// Internal getter for Chat to access the underlying tool
  nobodywho.RustTool get _internalTool => _tool;
}

/// A stream of response tokens from the model.
/// Implements [Stream<String>] so it can be used with `await for`.
class TokenStream extends Stream<String> {
  final nobodywho.RustTokenStream _tokenStream;

  TokenStream._(this._tokenStream);

  @override
  StreamSubscription<String> listen(
    void Function(String event)? onData, {
    Function? onError,
    void Function()? onDone,
    bool? cancelOnError,
  }) {
    return _generateStream().listen(
      onData,
      onError: onError,
      onDone: onDone,
      cancelOnError: cancelOnError,
    );
  }

  Stream<String> _generateStream() async* {
    while (true) {
      final token = await _tokenStream.nextToken();
      if (token == null) break;
      yield token;
    }
  }

  /// Wait for the complete response and return it as a single string.
  Future<String> completed() => _tokenStream.completed();
}

// Wrapper for the RustChat class. This is necessary to use the functionality
// gained by wrapping RustTool and RustTokenStream.
class Chat {
  final nobodywho.RustChat _chat;

  /// Private constructor for wrapping an existing Chat
  Chat._(this._chat);

  /// Create chat from existing model.
  ///
  /// For vision/multimodal models, the model should be loaded with image ingestion enabled:
  /// ```dart
  /// final model = Model.load("model.gguf", imageIngestion: "mmproj.gguf");
  /// final chat = Chat(model: model);
  /// ```
  factory Chat({
    required nobodywho.Model model,
    String? systemPrompt,
    int contextSize = 4096,
    bool allowThinking = true,
    List<Tool> tools = const [],
    nobodywho.SamplerConfig? sampler,
  }) {
    final chat = nobodywho.RustChat(
      model: model,
      systemPrompt: systemPrompt,
      contextSize: contextSize,
      allowThinking: allowThinking,
      tools: tools.map((t) => t._internalTool).toList(),
      sampler: sampler,
    );
    return Chat._(chat);
  }

  /// Create chat directly from a model path.
  ///
  /// [imageIngestion] is an optional path to a `.mmproj` projection model file,
  /// required for vision/multimodal models (e.g. LLaVA, Qwen-VL).
  static Future<Chat> fromPath({
    required String modelPath,
    String? imageIngestion,
    String? systemPrompt,
    int contextSize = 4096,
    bool allowThinking = true,
    List<Tool> tools = const [],
    nobodywho.SamplerConfig? sampler,
    bool useGpu = true,
  }) async {
    final chat = await nobodywho.RustChat.fromPath(
      modelPath: modelPath,
      imageIngestion: imageIngestion,
      systemPrompt: systemPrompt,
      contextSize: contextSize,
      allowThinking: allowThinking,
      tools: tools.map((t) => t._internalTool).toList(),
      sampler: sampler,
      useGpu: useGpu,
    );
    return Chat._(chat);
  }

  /// Send a prompt and get a stream of response tokens.
  ///
  /// Accepts a [Prompt] which may contain text and/or image parts.
  /// Use [Prompt.text] for text-only prompts:
  /// ```dart
  /// chat.ask(Prompt.text("Hello!"))
  /// ```
  TokenStream ask(Prompt prompt) {
    return TokenStream._(
      _chat.askWithPrompt(parts: _convertPromptParts(prompt.parts)),
    );
  }

  /// Get the chat history.
  Future<List<nobodywho.Message>> getChatHistory() => _chat.getChatHistory();

  /// Set the chat history.
  Future<void> setChatHistory(List<nobodywho.Message> messages) =>
      _chat.setChatHistory(messages: messages);

  /// Reset the chat history.
  Future<void> resetHistory() => _chat.resetHistory();

  /// Reset the context with a new system prompt and tools.
  Future<void> resetContext({
    required String systemPrompt,
    required List<Tool> tools,
  }) => _chat.resetContext(
    systemPrompt: systemPrompt,
    tools: tools.map((t) => t._internalTool).toList(),
  );

  /// Set whether thinking/reasoning is allowed.
  Future<void> setAllowThinking(bool allowThinking) =>
      _chat.setAllowThinking(allowThinking: allowThinking);

  /// Set the sampler configuration.
  Future<void> setSamplerConfig(nobodywho.SamplerConfig samplerConfig) =>
      _chat.setSamplerConfig(samplerConfig: samplerConfig);

  /// Set the system prompt.
  Future<void> setSystemPrompt(String systemPrompt) =>
      _chat.setSystemPrompt(systemPrompt: systemPrompt);

  /// Set the available tools.
  Future<void> setTools(List<Tool> tools) =>
      _chat.setTools(tools: tools.map((t) => t._internalTool).toList());

  /// Stop the current generation.
  void stopGeneration() => _chat.stopGeneration();
}
