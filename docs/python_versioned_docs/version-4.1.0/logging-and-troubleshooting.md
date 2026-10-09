---
title: Logging and Troubleshooting
sidebar_position: 7
---

# Logging and troubleshooting

The python bindings for NobodyWho integrate with python's standard `logging` utilities.

In short, to enable debug logs:

```python
import logging
logging.basicConfig(level=logging.DEBUG)
# Or to enable further logs from llama.cpp:
# logging.basicConfig(level=1)
```

This can be useful for getting some insight into what the model is choosing to do and when.
For example when tool calls are made, when context shifting happens, etc.
