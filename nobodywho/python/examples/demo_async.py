import asyncio
import sys

import nobodywho

model = nobodywho.Model(sys.argv[1])
chat = nobodywho.Chat(
    model, system_prompt="You are a helpful assistant", allow_thinking=False
)


async def main_streaming():
    while True:
        prompt = input("\nPlease enter your prompt: ")
        token_stream = chat.send_message(prompt)
        while token := await token_stream.next_token():
            print(token, end="", flush=True)


async def main_complete():
    while True:
        prompt = input("\nPlease enter your prompt: ")
        print(await chat.send_message(prompt).collect())


asyncio.run(main_complete())
