import nobodywhopython
import asyncio

model = nobodywhopython.NobodyWhoModel("/home/hanshh/work/Qwen_Qwen3-4B-Q5_K_M.gguf")
weather = {"paris" : 22, "copenhagen" : 10, "berlin" : 15}
weather_tool = nobodywhopython.NobodyWhoTool("get_weather", "Get the current weather for a location", [("location", "string", "The city to get weather for")], lambda args: f'Weather in {args["location"]}: Sunny, {weather[args["location"].lower()]}°C')
python_tool = nobodywhopython.NobodyWhoTool("python_runner", "Execute a string of python code", [("script", "string", "The python code to run. Must be syntactically correct python code.")], lambda args: exec(args["script"]))
chat = nobodywhopython.NobodyWhoChat(model, system_prompt = "You are a helpful assistant")#,tools=[weather_tool])



async def main():
    while True: 
        prompt = input("\nPlease enter your prompt: ")
        # print(await chat.say_complete_async(prompt))
        token_stream = chat.say_stream(prompt)
        while token := await token_stream.next_token_async():
            print(token, end = '', flush = True)


asyncio.run(main())