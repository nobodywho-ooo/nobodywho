import React from 'react';
import CodeBlock from '@theme/CodeBlock';
import Layout from '@theme/Layout';
import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';
import Link from '@docusaurus/Link';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';

const skillInstallCommand = 'npx skills add https://github.com/nobodywho-ooo/nobodywho --skill nobodywho';

type Install = {title?: string; code: string; language: string};

// One tab per binding, most used first; keep in sync with the navbar order in docusaurus.config.ts.
// `id` names the icon at static/img/icons/lang/<id>.svg and the navbar item's class.
const bindings: {
  id: string;
  name: string;
  description: string;
  docs: string;
  install?: Install[];
  note?: React.ReactNode;
}[] = [
  {
    id: 'python',
    name: 'Python',
    description: 'Batteries-included bindings with sync and async APIs.',
    docs: '/python/',
    install: [{code: 'pip install nobodywho', language: 'bash'}],
  },
  {
    id: 'godot',
    name: 'Godot',
    description: 'GDExtension for Godot 4.x game projects.',
    docs: '/godot/',
    note: (
      <>
        Install from the{' '}
        <a href="https://godotengine.org/asset-library/asset/2886">Godot Asset Library</a> or{' '}
        <a href="https://github.com/nobodywho-ooo/nobodywho/releases">GitHub releases</a>.
      </>
    ),
  },
  {
    id: 'flutter',
    name: 'Flutter',
    description: 'Cross-platform plugin for Flutter on mobile and desktop.',
    docs: '/flutter/',
    install: [{code: 'flutter pub add nobodywho', language: 'bash'}],
  },
  {
    id: 'react-native',
    name: 'RN/Expo',
    description: 'Drop-in module for React Native and Expo on Android and iOS.',
    docs: '/react-native/',
    install: [
      {title: 'React Native', code: 'npm install react-native-nobodywho', language: 'bash'},
      {title: 'Expo', code: 'npx expo install react-native-nobodywho', language: 'bash'},
    ],
  },
  {
    id: 'swift',
    name: 'Swift',
    description: 'Native Swift package for macOS and iOS apps.',
    docs: '/swift/',
    note: (
      <>
        Add the package with Swift Package Manager:{' '}
        <a href="https://github.com/nobodywho-ooo/nobodywho-swift">github.com/nobodywho-ooo/nobodywho-swift</a>
      </>
    ),
  },
  {
    id: 'kotlin',
    name: 'Kotlin',
    description: 'Native Kotlin library for Android and desktop JVM apps.',
    docs: '/kotlin/',
    install: [{code: 'implementation("ai.nobodywho:nobodywho-android:2.2.0")', language: 'kotlin'}],
  },
  {
    id: 'csharp',
    name: 'C#',
    description: '.NET 10 library for desktop apps on Windows, Linux and macOS.',
    docs: '/csharp/',
    install: [{code: 'dotnet add package NobodyWho', language: 'bash'}],
  },
];

export default function Home(): React.JSX.Element {
  const {siteConfig} = useDocusaurusContext();

  return (
    <Layout title="Home" description={siteConfig.tagline}>
      <main className="nw-home">
        <header className="nw-phero">
          <p className="nw-eyebrow">Documentation</p>
          <h1>Local-first LLM inference for your apps.</h1>
          <div className="nw-phero__skill">
            <p className="nw-phero__skill-label">Using an AI coding agent? Give it the NobodyWho skill:</p>
            <div className="skill-install-command">
              <CodeBlock language="bash">{skillInstallCommand}</CodeBlock>
            </div>
          </div>
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
          </div>
        </section>

        <section className="nw-home__section">
          <div className="nw-head">
            <h2>Choose your binding.</h2>
          </div>
          <div className="nw-install">
            <Tabs className="nw-install__tabs">
              {bindings.map((b) => (
                <TabItem
                  key={b.id}
                  value={b.id}
                  label={b.name}
                  attributes={{className: `nw-install__tab lang-icon lang-icon--${b.id}`}}
                >
                  <div className="nw-install__panel">
                    <p className="nw-install__desc">{b.description}</p>
                    {b.install?.map((i) => (
                      <CodeBlock key={i.code} language={i.language} title={i.title}>
                        {i.code}
                      </CodeBlock>
                    ))}
                    {b.note && <p className="nw-install__note">{b.note}</p>}
                    <Link to={b.docs} className="nw-more nw-more--accent">
                      {b.id === 'react-native' ? 'Read the React Native and Expo docs' : `Read the ${b.name} docs`}
                    </Link>
                  </div>
                </TabItem>
              ))}
            </Tabs>
          </div>
        </section>
      </main>
    </Layout>
  );
}
