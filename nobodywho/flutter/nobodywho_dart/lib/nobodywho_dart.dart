library;

export 'src/rust/api/nobodywho.dart';
export 'src/rust/frb_generated.dart' show RustLib;

import 'dart:async';
import 'dart:convert';
import 'src/rust/api/nobodywho.dart';

NobodyWhoTool describeTool({
  required Function function,
  required String name,
  required String description
}) {
  // narrow wrapper needs to be written in dart to access `function.runtimeType`
  // and to deal with dynamic function parameters

  // make it a String -> String function
  final wrappedfunction = (String jsonString) {
    Map<String, dynamic> jsonMap = json.decode(jsonString);
    Map<Symbol, dynamic> namedParams = Map.fromEntries(
      jsonMap.entries.map((e) => MapEntry(Symbol(e.key), e.value))
    );
    
    final result = Function.apply(function, [], namedParams).toString();

    // TODO: await
    return result;
  };

  return newToolImpl(
    function: wrappedfunction,
    name: name,
    description: description,
    runtimeType: function.runtimeType.toString()
  );
}

