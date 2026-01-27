import 'dart:math';
import 'package:nobodywho_flutter/nobodywho_flutter.dart' as nobodywho;

final _random = Random();

/// Generate a random integer between min and max (inclusive).
int randomInt({required int min, required int max}) {
  if (min > max) {
    final temp = min;
    min = max;
    max = temp;
  }
  return min + _random.nextInt(max - min + 1);
}

/// Generate a random double between min and max.
double randomDouble({required num min, required num max}) {
  final minD = min.toDouble();
  final maxD = max.toDouble();
  if (minD > maxD) {
    return maxD + _random.nextDouble() * (minD - maxD);
  }
  return minD + _random.nextDouble() * (maxD - minD);
}

/// Generate a random boolean (coin flip).
/// The seed parameter is ignored (placeholder for required named parameter).
bool randomBool({String? seed}) {
  return _random.nextBool();
}

/// Pick a random item from a comma-separated list.
String randomChoice({required String items}) {
  final list = items.split(',').map((s) => s.trim()).where((s) => s.isNotEmpty).toList();
  if (list.isEmpty) return 'Error: No items provided';
  return list[_random.nextInt(list.length)];
}

/// Generate a random UUID-like string.
/// The version parameter is ignored (placeholder for required named parameter).
String randomUuid({String? version}) {
  const chars = '0123456789abcdef';
  String gen(int len) => List.generate(len, (_) => chars[_random.nextInt(16)]).join();
  return '${gen(8)}-${gen(4)}-${gen(4)}-${gen(4)}-${gen(12)}';
}

/// Roll dice (e.g., "2d6" for 2 six-sided dice).
String rollDice({required String dice}) {
  final match = RegExp(r'^(\d+)d(\d+)$').firstMatch(dice.toLowerCase().trim());
  if (match == null) {
    return 'Error: Invalid dice format. Use NdS format (e.g., 2d6 for 2 six-sided dice).';
  }

  final count = int.parse(match.group(1)!);
  final sides = int.parse(match.group(2)!);

  if (count < 1 || count > 100) return 'Error: Dice count must be 1-100';
  if (sides < 2 || sides > 1000) return 'Error: Dice sides must be 2-1000';

  final rolls = List.generate(count, (_) => 1 + _random.nextInt(sides));
  final total = rolls.reduce((a, b) => a + b);

  return 'Rolls: ${rolls.join(", ")} = $total';
}

/// Creates all random tools as a list.
List<nobodywho.Tool> createRandomTools() {
  return [
    nobodywho.Tool.create(
      function: randomInt,
      name: 'random_int',
      description: 'Generate a random integer between min and max (inclusive). Parameters: min (integer), max (integer).',
    ),
    nobodywho.Tool.create(
      function: randomDouble,
      name: 'random_double',
      description: 'Generate a random decimal number between min and max. Parameters: min (number), max (number).',
    ),
    nobodywho.Tool.create(
      function: randomBool,
      name: 'random_bool',
      description: 'Generate a random boolean (true or false, like a coin flip). No parameters required.',
    ),
    nobodywho.Tool.create(
      function: randomChoice,
      name: 'random_choice',
      description: 'Pick a random item from a comma-separated list. Parameters: items (comma-separated string, e.g., "apple, banana, orange").',
    ),
    nobodywho.Tool.create(
      function: randomUuid,
      name: 'random_uuid',
      description: 'Generate a random UUID-like identifier. No parameters required.',
    ),
    nobodywho.Tool.create(
      function: rollDice,
      name: 'roll_dice',
      description: 'Roll dice in NdS format (N dice with S sides). Parameters: dice (string like "2d6" for 2 six-sided dice). Returns individual rolls and total.',
    ),
  ];
}
