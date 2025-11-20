import sys

import nobodywho

model = nobodywho.Model(sys.argv[1])
chat = nobodywho.Chat(
    model, system_prompt="You are a helpful assistant", allow_thinking=True
)


def main_streaming():
    while True:
        prompt = input("\nPlease enter your prompt: ")
        token_stream = chat.say_stream(prompt)
        while token := token_stream.next_token():
            print(token, end="", flush=True)


def main_complete():
    while True:
        prompt = input("\nPlease enter your prompt: ")
        print(chat.say_complete(prompt))


main_complete()
