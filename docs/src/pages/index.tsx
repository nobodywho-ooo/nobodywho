import React from 'react';
import CodeBlock from '@theme/CodeBlock';
import Layout from '@theme/Layout';
import Link from '@docusaurus/Link';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';

const skillInstallCommand = 'npx skills add https://github.com/nobodywho-ooo/nobodywho --skill nobodywho';

const sdks = [
  {
    name: 'Python',
    description: 'Batteries-included bindings with sync and async APIs.',
    install: 'pip install nobodywho',
    language: 'bash',
    link: '/python/',
  },
  {
    name: 'Swift',
    description: 'Native Swift package for macOS and iOS apps.',
    install: 'Swift Package Manager',
    link: '/swift/',
  },
  {
    name: 'React Native',
    description: 'Drop-in module for React Native on Android and iOS.',
    install: 'npm install react-native-nobodywho',
    language: 'bash',
    link: '/react-native/',
  },
  {
    name: 'Expo',
    description: 'Drop-in module for Expo on Android and iOS.',
    install: 'npx expo install react-native-nobodywho',
    language: 'bash',
    link: '/react-native/',
  },
  {
    name: 'Flutter',
    description: 'Cross-platform plugin for Flutter on mobile and desktop.',
    install: 'flutter pub add nobodywho',
    language: 'bash',
    link: '/flutter/',
  },
  {
    name: 'Godot',
    description: 'GDExtension for Godot 4.x game projects.',
    install: 'Asset Library or GitHub release',
    link: '/godot/',
  },
  {
    name: 'Kotlin',
    description: 'Native Kotlin library for Android and desktop JVM apps.',
    install: 'implementation("ai.nobodywho:nobodywho-android:2.2.0")',
    language: 'kotlin',
    link: '/kotlin/'
  }
];

export default function Home(): React.JSX.Element {
  const {siteConfig} = useDocusaurusContext();

  return (
    <Layout title="Home" description={siteConfig.tagline}>
      <main className="nw-home">
        <header className="nw-phero">
          <p className="nw-eyebrow">Documentation</p>
          <h1>Local-first LLM inference for your apps.</h1>
          <p className="nw-phero__intro">
            Run open-weight language models directly inside your software.
            Streaming chat, tool calling, structured output, embeddings, text to speech, speech to text and RAG.
            All offline with GPU acceleration. No servers, no API keys, no
            Docker. Built on{' '}
            <a href="https://github.com/ggml-org/llama.cpp" target="_blank" rel="noreferrer">
              llama.cpp
            </a>.
          </p>
        </header>

        <section className="nw-home__section">
          <div className="nw-head">
            <h2>Get started.</h2>
          </div>
          <div className="nw-rows">
            <div className="nw-row">
              <h3>New to local LLMs</h3>
              <div>
                <p>
                  Start here if you are new to running language models locally. These
                  guides cover the core concepts — what models are, how to pick
                  one, and how quantization works.
                </p>
                <div className="nw-arrow-links">
                  <Link to="/docs/llm-basics" className="nw-more nw-more--accent">LLM basics</Link>
                  <Link to="/docs/model-selection" className="nw-more nw-more--accent">Model selection</Link>
                </div>
              </div>
            </div>
            <div className="nw-row">
              <h3>Using an AI coding agent</h3>
              <div>
                <p>
                  Install the NobodyWho skill so your agent can look up the current APIs and documentation.
                </p>
                <div className="skill-install-command">
                  <CodeBlock language="bash">{skillInstallCommand}</CodeBlock>
                </div>
              </div>
            </div>
          </div>
        </section>

        <section className="nw-home__section">
          <div className="nw-head">
            <h2>Choose your binding.</h2>
          </div>
          <ul className="nw-linklist nw-bindings">
            {sdks.map((sdk) => (
              <li key={sdk.name}>
                <Link to={sdk.link}>
                  <b>{sdk.name}</b>
                  <span className="nw-linklist__desc">{sdk.description}</span>
                </Link>
                {sdk.language ? (
                  <CodeBlock language={sdk.language}>{sdk.install}</CodeBlock>
                ) : (
                  <span className="nw-bindings__note">{sdk.install}</span>
                )}
              </li>
            ))}
          </ul>
        </section>
      </main>
    </Layout>
  );
}
