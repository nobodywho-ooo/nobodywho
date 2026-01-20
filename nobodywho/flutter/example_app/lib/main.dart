import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import 'package:nobodywho_flutter/nobodywho_flutter.dart' as nobodywho;

import 'app.dart';
import 'models/app_state.dart';

void main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await nobodywho.NobodyWho.init();
  runApp(
    ChangeNotifierProvider(
      create: (_) => AppState(),
      child: const ShowcaseApp(),
    ),
  );
}
