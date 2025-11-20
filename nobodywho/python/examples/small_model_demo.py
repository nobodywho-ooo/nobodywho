import sys

import nobodywho

model = nobodywho.NobodyWhoModel(sys.argv[1])
chat = nobodywho.NobodyWhoChat(
    model, system_prompt="You are a helpful assistant", allow_thinking=False
)


result = chat.say_complete("What is the capital of Denmark?")
print(result)
assert "copenhagen" in result.lower(), "Model does not know the capital of Denmark."
