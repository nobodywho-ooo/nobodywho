import 'package:flutter/foundation.dart';
import 'package:nobodywho_flutter/nobodywho_flutter.dart' as nobodywho;

import '../tools/calculator_tool.dart';
import '../tools/weather_tool.dart';
import '../tools/datetime_tool.dart';
import '../tools/string_tool.dart';
import '../tools/random_tool.dart';
import '../tools/converter_tool.dart';

/// Tool category for grouping in UI.
enum ToolCategory {
  calculator('Calculator', 'Basic arithmetic operations'),
  datetime('Date & Time', 'Date and time utilities'),
  string('String', 'Text manipulation tools'),
  random('Random', 'Random number generation'),
  converter('Unit Converter', 'Unit conversion utilities'),
  weather('Weather', 'Weather information');

  final String label;
  final String description;
  const ToolCategory(this.label, this.description);
}

/// Available tool definitions that can be selected.
enum AvailableTool {
  // Calculator tools
  calculatorAdd('Add', 'Add two numbers together', ToolCategory.calculator),
  calculatorSubtract('Subtract', 'Subtract two numbers', ToolCategory.calculator),
  calculatorMultiply('Multiply', 'Multiply two numbers', ToolCategory.calculator),
  calculatorDivide('Divide', 'Divide two numbers', ToolCategory.calculator),

  // Date/Time tools
  getCurrentDateTime('Current DateTime', 'Get current date and time in ISO format', ToolCategory.datetime),
  getCurrentDate('Current Date', 'Get current date (YYYY-MM-DD)', ToolCategory.datetime),
  getCurrentTime('Current Time', 'Get current time (HH:MM:SS)', ToolCategory.datetime),
  getDayOfWeek('Day of Week', 'Get the current day name', ToolCategory.datetime),
  daysBetween('Days Between', 'Calculate days between two dates', ToolCategory.datetime),
  addDaysToDate('Add Days', 'Add days to a date', ToolCategory.datetime),

  // String tools
  toUppercase('Uppercase', 'Convert text to uppercase', ToolCategory.string),
  toLowercase('Lowercase', 'Convert text to lowercase', ToolCategory.string),
  reverseString('Reverse', 'Reverse a string', ToolCategory.string),
  stringLength('Length', 'Get string length', ToolCategory.string),
  countWords('Count Words', 'Count words in text', ToolCategory.string),
  replaceText('Replace', 'Replace text in string', ToolCategory.string),
  trimText('Trim', 'Trim whitespace', ToolCategory.string),
  repeatString('Repeat', 'Repeat string N times', ToolCategory.string),

  // Random tools
  randomInt('Random Int', 'Random integer in range', ToolCategory.random),
  randomDouble('Random Double', 'Random decimal in range', ToolCategory.random),
  randomBool('Random Bool', 'Random true/false', ToolCategory.random),
  randomChoice('Random Choice', 'Pick random from list', ToolCategory.random),
  randomUuid('Random UUID', 'Generate UUID', ToolCategory.random),
  rollDice('Roll Dice', 'Roll dice (e.g., 2d6)', ToolCategory.random),

  // Converter tools
  celsiusToFahrenheit('C to F', 'Celsius to Fahrenheit', ToolCategory.converter),
  fahrenheitToCelsius('F to C', 'Fahrenheit to Celsius', ToolCategory.converter),
  kmToMiles('km to mi', 'Kilometers to miles', ToolCategory.converter),
  milesToKm('mi to km', 'Miles to kilometers', ToolCategory.converter),
  kgToLbs('kg to lbs', 'Kilograms to pounds', ToolCategory.converter),
  lbsToKg('lbs to kg', 'Pounds to kilograms', ToolCategory.converter),
  metersToFeet('m to ft', 'Meters to feet', ToolCategory.converter),
  feetToMeters('ft to m', 'Feet to meters', ToolCategory.converter),
  litersToGallons('L to gal', 'Liters to gallons', ToolCategory.converter),
  gallonsToLiters('gal to L', 'Gallons to liters', ToolCategory.converter),
  degreesToRadians('deg to rad', 'Degrees to radians', ToolCategory.converter),
  radiansToDegrees('rad to deg', 'Radians to degrees', ToolCategory.converter),

  // Weather tool
  weather('Weather', 'Get weather for a city', ToolCategory.weather);

  final String label;
  final String description;
  final ToolCategory category;
  const AvailableTool(this.label, this.description, this.category);
}

/// Shared application state for the setup wizard and chat.
class AppState extends ChangeNotifier {
  // Step 1: Model
  String? _modelPath;
  bool _useGpu = true;
  bool _isModelLoading = false;
  String? _loadError;

  // Step 2: Tools
  final Set<AvailableTool> _selectedTools = {};

  // Step 3: Sampler
  nobodywho.SamplerConfig? _samplerConfig;
  String _samplerDescription = 'Default';

  // Step 4: System prompt
  String _systemPrompt = 'You are a helpful assistant.';
  int _contextSize = 4096;
  bool _allowThinking = false;

  // Final: Loaded model and chat
  nobodywho.Model? _model;
  nobodywho.Chat? _chat;

  // --- Getters ---

  String? get modelPath => _modelPath;
  String? get modelName =>
      _modelPath?.split('/').last ?? _modelPath?.split('\\').last;
  bool get useGpu => _useGpu;
  bool get isModelLoading => _isModelLoading;
  String? get loadError => _loadError;

  Set<AvailableTool> get selectedTools => Set.unmodifiable(_selectedTools);
  bool isToolSelected(AvailableTool tool) => _selectedTools.contains(tool);

  nobodywho.SamplerConfig? get samplerConfig => _samplerConfig;
  String get samplerDescription => _samplerDescription;

  String get systemPrompt => _systemPrompt;
  int get contextSize => _contextSize;
  bool get allowThinking => _allowThinking;

  nobodywho.Model? get model => _model;
  nobodywho.Chat? get chat => _chat;
  bool get isReady => _chat != null;

  // --- Step 1: Model Selection ---

  void setModelPath(String path) {
    _modelPath = path;
    _loadError = null;
    notifyListeners();
  }

  void setUseGpu(bool value) {
    _useGpu = value;
    notifyListeners();
  }

  // --- Step 2: Tool Selection ---

  void toggleTool(AvailableTool tool) {
    if (_selectedTools.contains(tool)) {
      _selectedTools.remove(tool);
    } else {
      _selectedTools.add(tool);
    }
    notifyListeners();
  }

  void selectAllTools() {
    _selectedTools.addAll(AvailableTool.values);
    notifyListeners();
  }

  void clearToolSelection() {
    _selectedTools.clear();
    notifyListeners();
  }

  void selectCategory(ToolCategory category) {
    for (final tool in AvailableTool.values) {
      if (tool.category == category) {
        _selectedTools.add(tool);
      }
    }
    notifyListeners();
  }

  void deselectCategory(ToolCategory category) {
    _selectedTools.removeWhere((tool) => tool.category == category);
    notifyListeners();
  }

  bool isCategoryFullySelected(ToolCategory category) {
    return AvailableTool.values
        .where((t) => t.category == category)
        .every((t) => _selectedTools.contains(t));
  }

  bool isCategoryPartiallySelected(ToolCategory category) {
    final categoryTools = AvailableTool.values.where((t) => t.category == category);
    final selectedCount = categoryTools.where((t) => _selectedTools.contains(t)).length;
    return selectedCount > 0 && selectedCount < categoryTools.length;
  }

  // --- Step 3: Sampler Configuration ---

  void setSamplerConfig(nobodywho.SamplerConfig config, String description) {
    _samplerConfig = config;
    _samplerDescription = description;
    notifyListeners();
  }

  void clearSamplerConfig() {
    _samplerConfig = null;
    _samplerDescription = 'Default';
    notifyListeners();
  }

  // --- Step 4: Chat Configuration ---

  void setSystemPrompt(String prompt) {
    _systemPrompt = prompt;
    notifyListeners();
  }

  void setContextSize(int size) {
    _contextSize = size;
    notifyListeners();
  }

  void setAllowThinking(bool value) {
    _allowThinking = value;
    notifyListeners();
  }

  // --- Final: Load Model and Create Chat ---

  List<nobodywho.Tool> _buildSelectedTools() {
    final tools = <nobodywho.Tool>[];

    // Calculator tools
    if (_selectedTools.contains(AvailableTool.calculatorAdd)) {
      tools.add(nobodywho.describeTool(
        function: add,
        name: 'calculator_add',
        description: 'Add two numbers together. Parameters: a (first number), b (second number). Returns the sum.',
      ));
    }
    if (_selectedTools.contains(AvailableTool.calculatorSubtract)) {
      tools.add(nobodywho.describeTool(
        function: subtract,
        name: 'calculator_subtract',
        description: 'Subtract the second number from the first. Parameters: a (first number), b (second number). Returns a - b.',
      ));
    }
    if (_selectedTools.contains(AvailableTool.calculatorMultiply)) {
      tools.add(nobodywho.describeTool(
        function: multiply,
        name: 'calculator_multiply',
        description: 'Multiply two numbers. Parameters: a (first number), b (second number). Returns the product.',
      ));
    }
    if (_selectedTools.contains(AvailableTool.calculatorDivide)) {
      tools.add(nobodywho.describeTool(
        function: divide,
        name: 'calculator_divide',
        description: 'Divide the first number by the second. Parameters: a (dividend), b (divisor). Returns a / b or an error if dividing by zero.',
      ));
    }

    // Date/Time tools
    if (_selectedTools.contains(AvailableTool.getCurrentDateTime)) {
      tools.add(nobodywho.describeTool(
        function: getCurrentDateTime,
        name: 'get_current_datetime',
        description: 'Get the current date and time in ISO 8601 format. Optional: timezone (string, ignored).',
      ));
    }
    if (_selectedTools.contains(AvailableTool.getCurrentDate)) {
      tools.add(nobodywho.describeTool(
        function: getCurrentDate,
        name: 'get_current_date',
        description: 'Get the current date in YYYY-MM-DD format. Optional: timezone (string, ignored).',
      ));
    }
    if (_selectedTools.contains(AvailableTool.getCurrentTime)) {
      tools.add(nobodywho.describeTool(
        function: getCurrentTime,
        name: 'get_current_time',
        description: 'Get the current time in HH:MM:SS format. Optional: timezone (string, ignored).',
      ));
    }
    if (_selectedTools.contains(AvailableTool.getDayOfWeek)) {
      tools.add(nobodywho.describeTool(
        function: getDayOfWeek,
        name: 'get_day_of_week',
        description: 'Get the current day of the week (e.g., Monday, Tuesday). Optional: timezone (string, ignored).',
      ));
    }
    if (_selectedTools.contains(AvailableTool.daysBetween)) {
      tools.add(nobodywho.describeTool(
        function: daysBetween,
        name: 'days_between',
        description: 'Calculate the number of days between two dates. Parameters: startDate (YYYY-MM-DD), endDate (YYYY-MM-DD). Returns the number of days.',
      ));
    }
    if (_selectedTools.contains(AvailableTool.addDaysToDate)) {
      tools.add(nobodywho.describeTool(
        function: addDaysToDate,
        name: 'add_days_to_date',
        description: 'Add or subtract days from a date. Parameters: date (YYYY-MM-DD), days (integer, can be negative). Returns the resulting date.',
      ));
    }

    // String tools
    if (_selectedTools.contains(AvailableTool.toUppercase)) {
      tools.add(nobodywho.describeTool(
        function: toUppercase,
        name: 'to_uppercase',
        description: 'Convert a string to uppercase. Parameters: text (the string to convert).',
      ));
    }
    if (_selectedTools.contains(AvailableTool.toLowercase)) {
      tools.add(nobodywho.describeTool(
        function: toLowercase,
        name: 'to_lowercase',
        description: 'Convert a string to lowercase. Parameters: text (the string to convert).',
      ));
    }
    if (_selectedTools.contains(AvailableTool.reverseString)) {
      tools.add(nobodywho.describeTool(
        function: reverseString,
        name: 'reverse_string',
        description: 'Reverse a string. Parameters: text (the string to reverse).',
      ));
    }
    if (_selectedTools.contains(AvailableTool.stringLength)) {
      tools.add(nobodywho.describeTool(
        function: stringLength,
        name: 'string_length',
        description: 'Get the length of a string. Parameters: text (the string to measure). Returns the number of characters.',
      ));
    }
    if (_selectedTools.contains(AvailableTool.countWords)) {
      tools.add(nobodywho.describeTool(
        function: countWords,
        name: 'count_words',
        description: 'Count the number of words in a string. Parameters: text (the string to count words in).',
      ));
    }
    if (_selectedTools.contains(AvailableTool.replaceText)) {
      tools.add(nobodywho.describeTool(
        function: replaceText,
        name: 'replace_text',
        description: 'Replace all occurrences of a substring. Parameters: text (original string), find (substring to find), replacement (string to replace with).',
      ));
    }
    if (_selectedTools.contains(AvailableTool.trimText)) {
      tools.add(nobodywho.describeTool(
        function: trimText,
        name: 'trim_text',
        description: 'Remove leading and trailing whitespace from a string. Parameters: text (the string to trim).',
      ));
    }
    if (_selectedTools.contains(AvailableTool.repeatString)) {
      tools.add(nobodywho.describeTool(
        function: repeatString,
        name: 'repeat_string',
        description: 'Repeat a string N times. Parameters: text (string to repeat), times (number of repetitions, max 100).',
      ));
    }

    // Random tools
    if (_selectedTools.contains(AvailableTool.randomInt)) {
      tools.add(nobodywho.describeTool(
        function: randomInt,
        name: 'random_int',
        description: 'Generate a random integer between min and max (inclusive). Parameters: min (integer), max (integer).',
      ));
    }
    if (_selectedTools.contains(AvailableTool.randomDouble)) {
      tools.add(nobodywho.describeTool(
        function: randomDouble,
        name: 'random_double',
        description: 'Generate a random decimal number between min and max. Parameters: min (number), max (number).',
      ));
    }
    if (_selectedTools.contains(AvailableTool.randomBool)) {
      tools.add(nobodywho.describeTool(
        function: randomBool,
        name: 'random_bool',
        description: 'Generate a random boolean (true or false, like a coin flip). Optional: seed (string, ignored).',
      ));
    }
    if (_selectedTools.contains(AvailableTool.randomChoice)) {
      tools.add(nobodywho.describeTool(
        function: randomChoice,
        name: 'random_choice',
        description: 'Pick a random item from a comma-separated list. Parameters: items (comma-separated string, e.g., "apple, banana, orange").',
      ));
    }
    if (_selectedTools.contains(AvailableTool.randomUuid)) {
      tools.add(nobodywho.describeTool(
        function: randomUuid,
        name: 'random_uuid',
        description: 'Generate a random UUID-like identifier. Optional: version (string, ignored).',
      ));
    }
    if (_selectedTools.contains(AvailableTool.rollDice)) {
      tools.add(nobodywho.describeTool(
        function: rollDice,
        name: 'roll_dice',
        description: 'Roll dice in NdS format (N dice with S sides). Parameters: dice (string like "2d6" for 2 six-sided dice). Returns individual rolls and total.',
      ));
    }

    // Converter tools
    if (_selectedTools.contains(AvailableTool.celsiusToFahrenheit)) {
      tools.add(nobodywho.describeTool(
        function: celsiusToFahrenheit,
        name: 'celsius_to_fahrenheit',
        description: 'Convert temperature from Celsius to Fahrenheit. Parameters: celsius (number).',
      ));
    }
    if (_selectedTools.contains(AvailableTool.fahrenheitToCelsius)) {
      tools.add(nobodywho.describeTool(
        function: fahrenheitToCelsius,
        name: 'fahrenheit_to_celsius',
        description: 'Convert temperature from Fahrenheit to Celsius. Parameters: fahrenheit (number).',
      ));
    }
    if (_selectedTools.contains(AvailableTool.kmToMiles)) {
      tools.add(nobodywho.describeTool(
        function: kmToMiles,
        name: 'km_to_miles',
        description: 'Convert distance from kilometers to miles. Parameters: km (number).',
      ));
    }
    if (_selectedTools.contains(AvailableTool.milesToKm)) {
      tools.add(nobodywho.describeTool(
        function: milesToKm,
        name: 'miles_to_km',
        description: 'Convert distance from miles to kilometers. Parameters: miles (number).',
      ));
    }
    if (_selectedTools.contains(AvailableTool.kgToLbs)) {
      tools.add(nobodywho.describeTool(
        function: kgToLbs,
        name: 'kg_to_lbs',
        description: 'Convert weight from kilograms to pounds. Parameters: kg (number).',
      ));
    }
    if (_selectedTools.contains(AvailableTool.lbsToKg)) {
      tools.add(nobodywho.describeTool(
        function: lbsToKg,
        name: 'lbs_to_kg',
        description: 'Convert weight from pounds to kilograms. Parameters: lbs (number).',
      ));
    }
    if (_selectedTools.contains(AvailableTool.metersToFeet)) {
      tools.add(nobodywho.describeTool(
        function: metersToFeet,
        name: 'meters_to_feet',
        description: 'Convert length from meters to feet. Parameters: meters (number).',
      ));
    }
    if (_selectedTools.contains(AvailableTool.feetToMeters)) {
      tools.add(nobodywho.describeTool(
        function: feetToMeters,
        name: 'feet_to_meters',
        description: 'Convert length from feet to meters. Parameters: feet (number).',
      ));
    }
    if (_selectedTools.contains(AvailableTool.litersToGallons)) {
      tools.add(nobodywho.describeTool(
        function: litersToGallons,
        name: 'liters_to_gallons',
        description: 'Convert volume from liters to US gallons. Parameters: liters (number).',
      ));
    }
    if (_selectedTools.contains(AvailableTool.gallonsToLiters)) {
      tools.add(nobodywho.describeTool(
        function: gallonsToLiters,
        name: 'gallons_to_liters',
        description: 'Convert volume from US gallons to liters. Parameters: gallons (number).',
      ));
    }
    if (_selectedTools.contains(AvailableTool.degreesToRadians)) {
      tools.add(nobodywho.describeTool(
        function: degreesToRadians,
        name: 'degrees_to_radians',
        description: 'Convert angle from degrees to radians. Parameters: degrees (number).',
      ));
    }
    if (_selectedTools.contains(AvailableTool.radiansToDegrees)) {
      tools.add(nobodywho.describeTool(
        function: radiansToDegrees,
        name: 'radians_to_degrees',
        description: 'Convert angle from radians to degrees. Parameters: radians (number).',
      ));
    }

    // Weather tool
    if (_selectedTools.contains(AvailableTool.weather)) {
      tools.add(createWeatherTool());
    }

    return tools;
  }

  Future<bool> loadModelAndCreateChat() async {
    if (_modelPath == null) {
      _loadError = 'No model path selected';
      notifyListeners();
      return false;
    }

    _isModelLoading = true;
    _loadError = null;
    notifyListeners();

    try {
      // Load model (async)
      _model = await nobodywho.Model.load(modelPath: _modelPath!, useGpu: _useGpu);

      // Build tools
      final tools = _buildSelectedTools();

      // Create chat
      _chat = nobodywho.Chat(
        model: _model!,
        systemPrompt: _systemPrompt,
        contextSize: _contextSize,
        allowThinking: _allowThinking,
        tools: tools,
        sampler: _samplerConfig,
      );

      _isModelLoading = false;
      notifyListeners();
      return true;
    } catch (e) {
      _loadError = e.toString();
      _model = null;
      _chat = null;
      _isModelLoading = false;
      notifyListeners();
      return false;
    }
  }

  /// Reset to start over.
  void reset() {
    _modelPath = null;
    _useGpu = true;
    _isModelLoading = false;
    _loadError = null;
    _selectedTools.clear();
    _samplerConfig = null;
    _samplerDescription = 'Default';
    _systemPrompt = 'You are a helpful assistant.';
    _contextSize = 4096;
    _allowThinking = false;
    _model = null;
    _chat = null;
    notifyListeners();
  }
}
