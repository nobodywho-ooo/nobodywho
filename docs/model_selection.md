# Model Selection Guide

---

Choosing the right language model can make or break your project. In general you want to go as small as possible while still
being able to solve yopur issues without making to grave errrors or sounding fake or stupid. 

There are a lot of models out there and many puitfalls to fall down into - we have not yet learned all of them, so we will update the list as we learn more
buty here are some general observations we have made:

- Instruction tuned models are generally better at your roleplay scenario than a roleplay trained model is. 
So unless you train a model on your specific data or it has been trained on data ideal to your usecase, 
you might want to go with an instruction tuned model for roleplay.
- If you have found a nice model, but it is too large: there are many quantized models out there, which cuts the intelligence of the model in return for less space.

## Finding Reliable Benchmarks

### Major Leaderboards

**[Hugging Face Open LLM Leaderboard](https://huggingface.co/spaces/open-llm-leaderboard/open_llm_leaderboard)**
- Benchmark appoach with: IFEval, MuSR, GPQA, MATH, BBH, and MMLU-Pro
- Focus on open-source models

**[LMSYS Chatbot Arena](https://huggingface.co/spaces/lmarena-ai/chatbot-arena-leaderboard)**
- Human preference rankings using Elo rating system
- Real conversation evaluation with user voting
- Includes both open and closed models

**[GPU Poor LLM Arena](https://huggingface.co/spaces/k-mktr/gpu-poor-llm-arena)**
- Humasn preference rankings using an elo system
- maxed out at 14B models 


### Relevant Benchmarks

- **HumanEval**: HumanEval is a benchmark dataset developed by OpenAI that evaluates the performance of large language models (LLMs) in code generation tasks. This is not the same as human evaluation...
- **Instruction-Following Evaluation (IFEval)**: Abiltiy to follow and execute instructions.
- **Multistep Soft Reasoning (MuSR)**: Long text understanding.
- **Big Bench Hard (BBH)**: A set of challenges in language understanding, mathemathical reasoning and common sense

---

*Need help choosing between specific models? Check our [community Discord](https://discord.gg/nobodywho).* 