---
section: changed
bindings:
  kotlin: minor
  swift: minor
  python: minor
  react-native: minor
  flutter: minor
---

Token streams now wait until you've consumed the previous token before constructing the next token, instead of running eagerly in a worker thread.

This is important for backpressure and reliability (it avoids consuming resources producing tokens if nothing is consuming them), but if you're sometimes doing a lot of blocking work in your `for await stream` loop, then you might want to change that.
