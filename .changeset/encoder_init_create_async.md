---
section: changed
bindings:
  python: major
---

Moved `EncoderAsync` and `CrossEncoderAsync` constructors to be `.create` methods, which are `async` and allows communicating errors during construction.
