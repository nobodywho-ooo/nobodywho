import 'package:nobodywho_flutter/nobodywho_flutter.dart' as nobodywho;

/// Calculator tool providing basic arithmetic operations.
/// Uses `num` to accept both int and double from JSON.

double add({required num a, required num b}) {
  return a.toDouble() + b.toDouble();
}

double subtract({required num a, required num b}) {
  return a.toDouble() - b.toDouble();
}

double multiply({required num a, required num b}) {
  return a.toDouble() * b.toDouble();
}

String divide({required num a, required num b}) {
  if (b == 0) {
    return 'Error: Cannot divide by zero';
  }
  return (a.toDouble() / b.toDouble()).toString();
}

/// Creates all calculator tools as a list.
List<nobodywho.Tool> createCalculatorTools() {
  return [
    nobodywho.Tool.create(
      function: add,
      name: 'calculator_add',
      description:
          'Add two numbers together. Parameters: a (first number), b (second number). Returns the sum.',
    ),
    nobodywho.Tool.create(
      function: subtract,
      name: 'calculator_subtract',
      description:
          'Subtract the second number from the first. Parameters: a (first number), b (second number). Returns a - b.',
    ),
    nobodywho.Tool.create(
      function: multiply,
      name: 'calculator_multiply',
      description:
          'Multiply two numbers. Parameters: a (first number), b (second number). Returns the product.',
    ),
    nobodywho.Tool.create(
      function: divide,
      name: 'calculator_divide',
      description:
          'Divide the first number by the second. Parameters: a (dividend), b (divisor). Returns a / b or an error if dividing by zero.',
    ),
  ];
}
