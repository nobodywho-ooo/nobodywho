import sys

import nobodywho

model = nobodywho.Model(sys.argv[1])
chat = nobodywho.Chat(
    model, system_prompt="You are a helpful assistant", allow_thinking=False
)


result = chat.ask("What is the capital of Denmark?").completed()
print(result)
assert "copenhagen" in result.lower(), "Model does not know the capital of Denmark."
