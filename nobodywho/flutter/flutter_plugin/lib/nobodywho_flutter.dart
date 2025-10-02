library;

export 'src/rust/api/nobodywho.dart';
export 'src/rust/api/simple.dart';
export 'src/rust/frb_generated.dart' show RustLib;

import 'dart:async';
import 'dart:convert';
import 'src/rust/api/nobodywho.dart';
import 'src/rust/api/simple.dart';
import 'src/rust/frb_generated.dart';

NobodyWhoTool toolFromFunction({required Function function, required String name, required String description}) {
  // narrow wrapper need to be written in dart to access `function.runtimeType`
  // and to deal with dynamic function parameters

  // make it a
  final wrappedfunction = (String jsonString) async {
    Map<String, dynamic> jsonMap = json.decode(jsonString);
    Map<Symbol, dynamic> namedParams = Map.fromEntries(
      jsonMap.entries.map((e) => MapEntry(Symbol(e.key), e.value))
    );
    
    final result = Function.apply(function, [], namedParams);

    if (result is Future) {
      return (await result).toString();
    } else {
      return result.toString();
    }
  };

  return newToolImpl(function: wrappedfunction, name: name, description: description, runtimeType: function.runtimeType.toString());
}

