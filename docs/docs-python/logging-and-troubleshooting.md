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

This can be useful for seeing when the model makes tool calls or shifts its context.

## Disable logging

Disable NobodyWho and llama.cpp logs before loading or using a model:

```python
import nobodywho

nobodywho.set_logging(enabled=False)
```

This works for text-only and multimodal models, including multimodal logs written directly to stderr. The setting applies to the whole process. Call `set_logging(enabled=True)` to restore the default.
