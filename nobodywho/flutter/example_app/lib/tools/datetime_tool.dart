import 'package:nobodywho/nobodywho.dart' as nobodywho;

/// Get the current date and time in ISO format.
/// The timezone parameter is ignored (placeholder for required named parameter).
String getCurrentDateTime() {
  return DateTime.now().toIso8601String();
}

/// Get the current date in YYYY-MM-DD format.
/// The timezone parameter is ignored (placeholder for required named parameter).
String getCurrentDate() {
  final now = DateTime.now();
  return '${now.year}-${now.month.toString().padLeft(2, '0')}-${now.day.toString().padLeft(2, '0')}';
}

/// Get the current time in HH:MM:SS format.
/// The timezone parameter is ignored (placeholder for required named parameter).
String getCurrentTime() {
  final now = DateTime.now();
  return '${now.hour.toString().padLeft(2, '0')}:${now.minute.toString().padLeft(2, '0')}:${now.second.toString().padLeft(2, '0')}';
}

/// Get the day of the week.
/// The timezone parameter is ignored (placeholder for required named parameter).
String getDayOfWeek() {
  final weekdays = ['Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday', 'Sunday'];
  return weekdays[DateTime.now().weekday - 1];
}

/// Calculate days between two dates (YYYY-MM-DD format).
String daysBetween({required String startDate, required String endDate}) {
  try {
    final start = DateTime.parse(startDate);
    final end = DateTime.parse(endDate);
    final difference = end.difference(start).inDays;
    return difference.toString();
  } catch (e) {
    return 'Error: Invalid date format. Use YYYY-MM-DD.';
  }
}

/// Add days to a date.
String addDaysToDate({required String date, required int days}) {
  try {
    final parsed = DateTime.parse(date);
    final result = parsed.add(Duration(days: days));
    return '${result.year}-${result.month.toString().padLeft(2, '0')}-${result.day.toString().padLeft(2, '0')}';
  } catch (e) {
    return 'Error: Invalid date format. Use YYYY-MM-DD.';
  }
}

/// Creates all date/time tools as a list.
List<nobodywho.Tool> createDateTimeTools() {
  return [
    nobodywho.Tool(
      function: getCurrentDateTime,
      name: 'get_current_datetime',
      description: 'Get the current date and time in ISO 8601 format. No parameters required.',
    ),
    nobodywho.Tool(
      function: getCurrentDate,
      name: 'get_current_date',
      description: 'Get the current date in YYYY-MM-DD format. No parameters required.',
    ),
    nobodywho.Tool(
      function: getCurrentTime,
      name: 'get_current_time',
      description: 'Get the current time in HH:MM:SS format. No parameters required.',
    ),
    nobodywho.Tool(
      function: getDayOfWeek,
      name: 'get_day_of_week',
      description: 'Get the current day of the week (e.g., Monday, Tuesday). No parameters required.',
    ),
    nobodywho.Tool(
      function: daysBetween,
      name: 'days_between',
      description: 'Calculate the number of days between two dates. Parameters: startDate (YYYY-MM-DD), endDate (YYYY-MM-DD). Returns the number of days.',
    ),
    nobodywho.Tool(
      function: addDaysToDate,
      name: 'add_days_to_date',
      description: 'Add or subtract days from a date. Parameters: date (YYYY-MM-DD), days (integer, can be negative). Returns the resulting date.',
    ),
  ];
}
