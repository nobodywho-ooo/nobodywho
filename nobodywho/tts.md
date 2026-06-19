# TTS: The good, the bad, and the ugly

Today, Text-to-Speech is landing in NobodyWho.
This is another step on our way to become do it all local model inference library.
We released support for Kokoro, with Piper, Chatterbox, and Røest coming over the summer, after we finish internal phase of testing.

TTS is wild west with many different architectures, and we cannot use our beloved llama.cpp for inference.
Thus, we had to develop our own solution.
There are several ways to do it. One can wrap existing implementations behind a common interface, using tensor library such as candle to spell out the forward passes, or leverage onnx to handle inference and backend support for us.
In the end, we decided to go with onnx for two main reasons.
The main reason to NOT go with existing implementations, was that we want have the same acclerator support regardless of the model. Conveniently, ort provides a beautiful and convenient rust wrapper around the C++ runtime, allowing us to support most of the backends that llama.cpp does (RIP metal, you will be missed).
Feature parity to upstream.

The backend support was the thing that broke the scales for us. onnx gives us ...
Moreover, we want to get on top of supporting new model releases, so relying on others implementing it for us is not something we want to do.

Also, we would like to faithfuly support the upstream as much as possible, which sometimes require custom glueing code (phoneme translations between espeak and misaki)

We are also picky about the dependencies, and already existing projects sometimes depend on c/cpp libs, something we are actively trying to avoid due to build issues on various platforms we support.

Candle vs onnx, and why we like candle but do not use it -- backend support

## The good
- pure rust with misaki-rs, espeak-ng-rs, hound
- (onnx cpp runtime) :(
- add some data on how long the synthesis takes
- piper, chatterbox, roest, omnivoice soon on God fr

## The bad
- onnx does not support metal, only via MLCORE
- torch.export does not really work

## The ugly
- some custom glueing code
- kokoro support -- english good, others limited
- bindings soon


If you know how to do these things better, like Copenhagen, and would like to get pay for improving it, give us a shot at jobs (at) nobodywho (dot) ai.
o
