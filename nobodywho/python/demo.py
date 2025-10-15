import python
import asyncio

model = python.NobodyWhoModel("/home/hanshh/work/Qwen_Qwen3-4B-Q5_K_M.gguf")
tool = python.NobodyWhoTool("get_weather", "Get the current weather for a location", [("location", "string", "The city to get weather for")], lambda args: f'Weather in : Sunny, 22°C')
chat = python.NobodyWhoChat(model, system_prompt = "You are a helpful assistant",tools=[tool])



async def main():
    while True: 
        prompt = input("\nPlease enter your prompt: ")
        token_stream = chat.say_stream(prompt)
        while token := await token_stream.next_token_async():
            print(token, end = '', flush = True)


asyncio.run(main())

