import python

model = python.NobodyWhoModel("/home/hanshh/work/google_gemma-3-4b-it-Q5_K_M.gguf")
chathandler = python.NobodyWhoChatBuilder(model).with_system_prompt("You are a helpful assistant").build()

while True: 
    prompt = input("Please enter your prompt: ")
    token_stream = chathandler.say_stream("\n" + prompt)
    while token := token_stream.next_token():
        print(token, end = '', flush = True)