library;

export 'src/rust/api/nobodywho.dart';
export 'src/rust/api/simple.dart';
export 'src/rust/frb_generated.dart' show RustLib;

import 'src/rust/api/nobodywho.dart';
import 'src/rust/api/simple.dart';
import 'src/rust/frb_generated.dart';
import 'dart:async';

NobodyWhoTool toolFromFunction({required FutureOr<String> Function(String) function, required String name, required String description}) {
  // narrow wrapper need to be written in dart to access `function.runtimeType`
  return newToolImpl(function: function, name: name, description: description, runtimeType: function.runtimeType.toString());
}

