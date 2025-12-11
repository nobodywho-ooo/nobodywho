# Model Selection Guide

---

Choosing the right language model can make or break your project. In general you want to go as small as possible while still having the capabilities you need for your application.

## TL;DR

If you just want a ~2GB chat model that works well, try [Qwen3 4B Q4_K_M](https://huggingface.co/Qwen/Qwen3-4B-GGUF/blob/main/Qwen3-4B-Q4_K_M.gguf).


## Which models are compatible with NobodyWho?

Broadly: almost anything in the `.gguf` file format.

For chatting, it should be an instruction-tuned GGUF file that includes a jinja2 chat template in the metadata.
This description fits the vast majority of GGUF files out there. If in doubt, try it. NobodyWho will throw you a descriptive error message if something is wrong.

For embeddings or cross-encoding, you need to use models specific for embedding or cross-encoding. They will be named as such. Although notice that cross-encoding models are sometimes called "reranking" models.


## Understanding model names

Model files have names that look something like this: `Qwen_Qwen3-0.6B-Q4_K_M.gguf`

Let's break it down.

- `Qwen` is the name of the organization that trained the model.
- `Qwen3` is the name of the model release.
- `0.6B` refers to the parameter count of the model, in billions of parameters. This model has 0.6 billion parameters (aka 600 million parameters).
- `Q4` refers to the quantization level, i.e. the number of bits per parameter.
- `K_M` refers to details about the quantization techniques used. Don't worry too much about this for now. `S` means faster and less precise, `L` means slower and more precise, `M` is medium both.


## Quantization

Quantization refers to the practice of reducing the number of bits per weight.
This can make the model faster and smaller, with a relatively small loss in response quality.

Generally speaking, you can used models quantized down the Q4 or Q5 levels (4 or 5 bits per weight respectively),
without losing *too much* accuracy.

Look at the plot below to get a feel for how quantization levels differ.
It shows the models' ability to predict text on the y-axis versus the number of bits per weight on the x-axis.

![Perplexity/Quantization curve](assets/quantcurve.png)

In general, it's preferable to use a model with more parameters and fewer bits per parameter, as compared to a model with fewer parameters and more bits per parameter.
Your results may vary.


## Estimating Memory Usage

The memory requirement of a model is roughly its parameter count multiplied by its quantization level.

Here's a few examples:

- 2B @ Q8 ~= 2GB
- 2B @ Q4 ~= 1GB
- 14B @ Q4 ~= 7GB
- 14B @ Q2 ~= 3.5GB
- ..and so on


## Comparing Models

There are many places online for comparing benchmark scores of different LLMs, here's a few of them:

**[LLM-Stats.com](https://llm-stats.com/)**
- Includes filters for open models and small models.
- Compares recent models on a few different benchmarks.

**[OpenEvals on huggingface](https://huggingface.co/spaces/OpenEvals/find-a-leaderboard)**
- A collection of benchmark leaderboards in different domains.
- Includes both inaccessible proprietary models and open models.

Remember that you need an open model, in order to be able to find a GGUF download and run it locally (e.g. Gemma is open, but Gemini isn't).


## Finding a GGUF Download

Once you have decided on an LLM you want to try, you can usually find it on one of the big HuggingFace pages:

- https://huggingface.co/bartowski
- https://huggingface.co/unsloth/models

You can also just search "<modelname> GGUF" in your favorite search engine.


---

*Need help choosing between specific models? Check our [community Discord](https://discord.gg/nobodywho).* 
