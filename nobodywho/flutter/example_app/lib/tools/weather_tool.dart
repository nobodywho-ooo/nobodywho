import 'package:nobodywho_flutter/nobodywho_flutter.dart' as nobodywho;

/// Mock weather tool that returns simulated weather data.
///
/// In a real application, this would call a weather API.

// Mock weather database
const Map<String, Map<String, dynamic>> _mockWeatherData = {
  'new york': {
    'temperature': 72,
    'condition': 'Partly Cloudy',
    'humidity': 65,
    'wind': '10 mph NW',
  },
  'london': {
    'temperature': 55,
    'condition': 'Rainy',
    'humidity': 85,
    'wind': '15 mph SW',
  },
  'tokyo': {
    'temperature': 68,
    'condition': 'Sunny',
    'humidity': 50,
    'wind': '5 mph E',
  },
  'paris': {
    'temperature': 60,
    'condition': 'Overcast',
    'humidity': 70,
    'wind': '8 mph N',
  },
  'sydney': {
    'temperature': 82,
    'condition': 'Sunny',
    'humidity': 45,
    'wind': '12 mph SE',
  },
  'berlin': {
    'temperature': 52,
    'condition': 'Cloudy',
    'humidity': 75,
    'wind': '7 mph W',
  },
  'san francisco': {
    'temperature': 64,
    'condition': 'Foggy',
    'humidity': 80,
    'wind': '14 mph W',
  },
  'los angeles': {
    'temperature': 78,
    'condition': 'Sunny',
    'humidity': 35,
    'wind': '6 mph SW',
  },
};

/// Get current weather for a city.
///
/// Returns formatted weather information or a message if city not found.
String getWeather({required String city}) {
  final normalizedCity = city.toLowerCase().trim();
  final weather = _mockWeatherData[normalizedCity];

  if (weather == null) {
    // Return plausible random weather for unknown cities
    return 'Weather for $city: Temperature: 65F, Condition: Clear, Humidity: 55%, Wind: 7 mph';
  }

  return 'Weather for $city: Temperature: ${weather['temperature']}F, '
      'Condition: ${weather['condition']}, '
      'Humidity: ${weather['humidity']}%, '
      'Wind: ${weather['wind']}';
}

/// Creates the weather tool.
nobodywho.Tool createWeatherTool() {
  return nobodywho.describeTool(
    function: getWeather,
    name: 'get_weather',
    description:
        'Get the current weather for a city. Parameter: city (name of the city). Returns temperature in Fahrenheit, conditions, humidity percentage, and wind information.',
  );
}
