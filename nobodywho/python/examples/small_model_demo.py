import sys

import nobodywho

model = nobodywho.Model(sys.argv[1])
chat = nobodywho.Chat(
    model, system_prompt="You are a helpful assistant", allow_thinking=False
)


result = chat.send_message("What is the capital of Denmark?").collect_blocking()
print(result)
assert "copenhagen" in result.lower(), "Model does not know the capital of Denmark."
