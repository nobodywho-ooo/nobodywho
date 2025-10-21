import nobodywhopython
import asyncio
import sys


model = nobodywhopython.NobodyWhoModel(sys.argv[1])
chat = nobodywhopython.NobodyWhoChat(model, system_prompt = "You are a helpful assistant")



async def main():

    result = await chat.say_complete_async("What is the capital of Denmark?")
    print(result)
    assert "copenhagen" in result.lower(), "Model does not know the capital of Denmark."
        


asyncio.run(main())