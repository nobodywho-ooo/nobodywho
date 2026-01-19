import 'dart:math';
import 'package:nobodywho_flutter/nobodywho_flutter.dart' as nobodywho;

/// Convert Celsius to Fahrenheit.
double celsiusToFahrenheit({required num celsius}) {
  return celsius.toDouble() * 9 / 5 + 32;
}

/// Convert Fahrenheit to Celsius.
double fahrenheitToCelsius({required num fahrenheit}) {
  return (fahrenheit.toDouble() - 32) * 5 / 9;
}

/// Convert kilometers to miles.
double kmToMiles({required num km}) {
  return km.toDouble() * 0.621371;
}

/// Convert miles to kilometers.
double milesToKm({required num miles}) {
  return miles.toDouble() * 1.60934;
}

/// Convert kilograms to pounds.
double kgToLbs({required num kg}) {
  return kg.toDouble() * 2.20462;
}

/// Convert pounds to kilograms.
double lbsToKg({required num lbs}) {
  return lbs.toDouble() / 2.20462;
}

/// Convert meters to feet.
double metersToFeet({required num meters}) {
  return meters.toDouble() * 3.28084;
}

/// Convert feet to meters.
double feetToMeters({required num feet}) {
  return feet.toDouble() / 3.28084;
}

/// Convert liters to gallons (US).
double litersToGallons({required num liters}) {
  return liters.toDouble() * 0.264172;
}

/// Convert gallons (US) to liters.
double gallonsToLiters({required num gallons}) {
  return gallons.toDouble() / 0.264172;
}

/// Convert degrees to radians.
double degreesToRadians({required num degrees}) {
  return degrees.toDouble() * pi / 180;
}

/// Convert radians to degrees.
double radiansToDegrees({required num radians}) {
  return radians.toDouble() * 180 / pi;
}

/// Creates all converter tools as a list.
List<nobodywho.Tool> createConverterTools() {
  return [
    nobodywho.describeTool(
      function: celsiusToFahrenheit,
      name: 'celsius_to_fahrenheit',
      description: 'Convert temperature from Celsius to Fahrenheit. Parameters: celsius (number).',
    ),
    nobodywho.describeTool(
      function: fahrenheitToCelsius,
      name: 'fahrenheit_to_celsius',
      description: 'Convert temperature from Fahrenheit to Celsius. Parameters: fahrenheit (number).',
    ),
    nobodywho.describeTool(
      function: kmToMiles,
      name: 'km_to_miles',
      description: 'Convert distance from kilometers to miles. Parameters: km (number).',
    ),
    nobodywho.describeTool(
      function: milesToKm,
      name: 'miles_to_km',
      description: 'Convert distance from miles to kilometers. Parameters: miles (number).',
    ),
    nobodywho.describeTool(
      function: kgToLbs,
      name: 'kg_to_lbs',
      description: 'Convert weight from kilograms to pounds. Parameters: kg (number).',
    ),
    nobodywho.describeTool(
      function: lbsToKg,
      name: 'lbs_to_kg',
      description: 'Convert weight from pounds to kilograms. Parameters: lbs (number).',
    ),
    nobodywho.describeTool(
      function: metersToFeet,
      name: 'meters_to_feet',
      description: 'Convert length from meters to feet. Parameters: meters (number).',
    ),
    nobodywho.describeTool(
      function: feetToMeters,
      name: 'feet_to_meters',
      description: 'Convert length from feet to meters. Parameters: feet (number).',
    ),
    nobodywho.describeTool(
      function: litersToGallons,
      name: 'liters_to_gallons',
      description: 'Convert volume from liters to US gallons. Parameters: liters (number).',
    ),
    nobodywho.describeTool(
      function: gallonsToLiters,
      name: 'gallons_to_liters',
      description: 'Convert volume from US gallons to liters. Parameters: gallons (number).',
    ),
    nobodywho.describeTool(
      function: degreesToRadians,
      name: 'degrees_to_radians',
      description: 'Convert angle from degrees to radians. Parameters: degrees (number).',
    ),
    nobodywho.describeTool(
      function: radiansToDegrees,
      name: 'radians_to_degrees',
      description: 'Convert angle from radians to degrees. Parameters: radians (number).',
    ),
  ];
}
