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

  // make it a String -> Future<String> function
  final wrappedfunction = (String jsonString) async {
    print("Inside dart wrappedfunction!");
    // decode the input string as json
    Map<String, dynamic> jsonMap = json.decode(jsonString);
    // make it a map of symbols, to make Function.apply happy
    Map<Symbol, dynamic> namedParams = Map.fromEntries(
      jsonMap.entries.map((e) => MapEntry(Symbol(e.key), e.value))
    );
    
    // call the function
    final result = Function.apply(function, [], namedParams);

    // handle async tools and return
    if (result is Future) {
      return (await result).toString();
    } else {
      return result.toString();
    }
  };

  return newToolImpl(
    function: wrappedfunction,
    name: name,
    description: description,
    runtimeType: function.runtimeType.toString()
  );
}

