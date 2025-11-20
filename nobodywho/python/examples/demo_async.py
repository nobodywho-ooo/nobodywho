import nobodywhopython
import asyncio
import sys



model = nobodywhopython.NobodyWhoModel(sys.argv[1])
chat = nobodywhopython.NobodyWhoChat(model, system_prompt = "You are a helpful assistant")



async def main_streaming():
    while True: 
        prompt = input("\nPlease enter your prompt: ")
        token_stream = chat.say_stream(prompt)
        while token := await token_stream.next_token_async():
            print(token, end = '', flush = True)

async def main_complete():
    while True: 
        prompt = input("\nPlease enter your prompt: ")
        print(await chat.say_complete_async(prompt))

asyncio.run(main_complete())