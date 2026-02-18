# Changelog

## nobodywho-flutter-v0.3.1-rc4

- Change MacOS and iOS releases to use dynamic linking

## nobodywho-flutter-v0.3.0

- Add support for tool parameters with composite types (e.g. List<List<int>>)
- Fix CI/CD for targets that depend on the XCFramework files (MacOS + iOS)

## nobodywho-flutter-v0.2.0

- Add option to provide descriptions for individual parameters in Tool constructor.
- Remove slow trigger_word grammar triggers, significantly speeding up generation of long messages when tools are present
- Default to add_bos=true if GGUF file does not specify

## nobodywho-flutter-v0.1.1

- Set up automated publishing from CI

## nobodywho-flutter-v0.1.0

- Initial release!
