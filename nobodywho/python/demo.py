import python

model = python.NobodyWhoModel("/home/hanshh/work/Qwen_Qwen3-4B-Q5_K_M.gguf")
tool = python.NobodyWhoTool("get_weather", "Get the current weather for a location", [("location", "string", "The city to get weather for")], lambda args: f'Weather in : Sunny, 22°C')
chat = python.NobodyWhoChat(model, system_prompt = "You are a helpful assistant",tools=[tool])

while True: 
    prompt = input("\nPlease enter your prompt: ")
    token_stream = chat.say_stream(prompt)
    while token := token_stream.next_token():
        print(token, end = '', flush = True)


print(python.function_test_call(lambda args: print(f'Weather in {args["location"]}: Sunny, 22°C'), {"location" : "Copenhagen"}))