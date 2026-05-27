---
title: Overview
sidebar_position: 0
---

import Link from '@docusaurus/Link';

## What is NobodyWho?

NobodyWho is a lightweight, open-source inference engine for running open-weights LLMs inside your software.
We provide a simple, efficient, offline and privacy forward way of interacting with LLMs. No infrastructure needed!

In short, if you want to run a LLM, and integrate it with [tools](/python/tool-calling), configure its output,
enable real-time streaming of tokens, or maybe use it for creation of embeddings, NobodyWho makes it easy.

All of this is enabled by [Llama.cpp](https://github.com/ggml-org/llama.cpp), while having nice, simple API.

No need to mess around with docker containers, GPU servers, API keys, etc. We make it easy to run local LLMs in Swift, Python, React Native, Flutter, Godot, and JavaScript (browser / Node)!

## Code documentation

If you are already familiar with the basics of LLMs we suggest you go straight to the documentation of your selected integration.

<div style={{display: 'flex', flexWrap: 'wrap', gap: '0.5rem', margin: '1.25rem 0', justifyContent: 'center'}}>
  <Link to="/python/" className="button button--secondary button--sm">Python</Link>
  <Link to="/swift/" className="button button--secondary button--sm">Swift</Link>
  <Link to="/react-native/" className="button button--secondary button--sm">React Native</Link>
  <Link to="/flutter/" className="button button--secondary button--sm">Flutter</Link>
  <Link to="/godot/install" className="button button--secondary button--sm">Godot</Link>
  <a href="https://github.com/nobodywho-ooo/nobodywho/blob/main/nobodywho/js/README.md" className="button button--secondary button--sm">JavaScript (wasm)</a>
</div>

## Basic LLM concepts

If you are unfamiliar with the basics of LLMs or are just interested we also provide a simple introduction to the most important concepts you need to know in order to get the most out of NobodyWho.
