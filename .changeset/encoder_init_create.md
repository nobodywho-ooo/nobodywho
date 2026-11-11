---
section: changed
bindings:
  kotlin: major
  flutter: major
  react-native: major
---

Moved `Encoder` and `CrossEncoder` constructors to be `.create` methods, which are `async` and allows communicating errors during construction.
