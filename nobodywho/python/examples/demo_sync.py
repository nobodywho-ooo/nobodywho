import sys

import nobodywho

model = nobodywho.Model(sys.argv[1])
chat = nobodywho.Chat(
    model, system_prompt="You are a helpful assistant", allow_thinking=True
)


def main_streaming():
    while True:
        prompt = input("\nPlease enter your prompt: ")
        token_stream = chat.ask(prompt)
        while token := token_stream.next_token_blocking():
            print(token, end="", flush=True)


def main_complete():
    while True:
        prompt = input("\nPlease enter your prompt: ")
        print(chat.ask(prompt).collect_blocking())


main_complete()
