import 'package:nobodywho_flutter/nobodywho_flutter.dart' as nobodywho;

/// Convert string to uppercase.
String toUppercase({required String text}) {
  return text.toUpperCase();
}

/// Convert string to lowercase.
String toLowercase({required String text}) {
  return text.toLowerCase();
}

/// Reverse a string.
String reverseString({required String text}) {
  return text.split('').reversed.join('');
}

/// Get the length of a string.
int stringLength({required String text}) {
  return text.length;
}

/// Count words in a string.
int countWords({required String text}) {
  if (text.trim().isEmpty) return 0;
  return text.trim().split(RegExp(r'\s+')).length;
}

/// Replace text in a string.
String replaceText({required String text, required String find, required String replacement}) {
  return text.replaceAll(find, replacement);
}

/// Trim whitespace from both ends.
String trimText({required String text}) {
  return text.trim();
}

/// Repeat a string N times.
String repeatString({required String text, required int times}) {
  if (times < 0) return 'Error: times must be non-negative';
  if (times > 100) return 'Error: times must be 100 or less';
  return text * times;
}

/// Creates all string tools as a list.
List<nobodywho.Tool> createStringTools() {
  return [
    nobodywho.Tool(
      function: toUppercase,
      name: 'to_uppercase',
      description: 'Convert a string to uppercase. Parameters: text (the string to convert).',
    ),
    nobodywho.Tool(
      function: toLowercase,
      name: 'to_lowercase',
      description: 'Convert a string to lowercase. Parameters: text (the string to convert).',
    ),
    nobodywho.Tool(
      function: reverseString,
      name: 'reverse_string',
      description: 'Reverse a string. Parameters: text (the string to reverse).',
    ),
    nobodywho.Tool(
      function: stringLength,
      name: 'string_length',
      description: 'Get the length of a string. Parameters: text (the string to measure). Returns the number of characters.',
    ),
    nobodywho.Tool(
      function: countWords,
      name: 'count_words',
      description: 'Count the number of words in a string. Parameters: text (the string to count words in).',
    ),
    nobodywho.Tool(
      function: replaceText,
      name: 'replace_text',
      description: 'Replace all occurrences of a substring. Parameters: text (original string), find (substring to find), replacement (string to replace with).',
    ),
    nobodywho.Tool(
      function: trimText,
      name: 'trim_text',
      description: 'Remove leading and trailing whitespace from a string. Parameters: text (the string to trim).',
    ),
    nobodywho.Tool(
      function: repeatString,
      name: 'repeat_string',
      description: 'Repeat a string N times. Parameters: text (string to repeat), times (number of repetitions, max 100).',
    ),
  ];
}
