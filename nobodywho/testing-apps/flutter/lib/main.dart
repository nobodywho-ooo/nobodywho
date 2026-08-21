import 'package:flutter/material.dart';

/// Host app for the on-device integration tests in `integration_test/`.
///
/// Deliberately minimal: the tests drive the NobodyWho API directly, so this
/// only needs to be a launchable Flutter app.
void main() => runApp(const NobodyWhoTestApp());

class NobodyWhoTestApp extends StatelessWidget {
  const NobodyWhoTestApp({super.key});

  @override
  Widget build(BuildContext context) => const MaterialApp(
        home: Scaffold(
          body: Center(child: Text('NobodyWho device test host')),
        ),
      );
}
