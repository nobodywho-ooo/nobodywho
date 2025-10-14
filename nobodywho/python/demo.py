import python

model = python.NobodyWhoModel("/home/hanshh/work/Qwen_Qwen3-4B-Q5_K_M.gguf")
chat = python.NobodyWhoChat(model, system_prompt = "You are a helpful assistant")

while True: 
    prompt = input("\nPlease enter your prompt: ")
    token_stream = chat.say_stream(prompt)
    while token := token_stream.next_token():
        print(token, end = '', flush = True)