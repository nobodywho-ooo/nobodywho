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
    link: '/react-native/',
  },
  {
    name: 'Expo',
    description: 'Drop-in module for Expo on Android and iOS.',
    install: 'npx expo install react-native-nobodywho',
    link: '/react-native/',
  },
  {
    name: 'Flutter',
    description: 'Cross-platform plugin for Flutter on mobile and desktop.',
    install: 'flutter pub add nobodywho',
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
    link: '/kotlin/'
  }
];

function SDKCard({name, description, install, link}: {
  name: string;
  description: string;
  install: string;
  link: string;
}) {
  return (
    <Link to={link} className="home-card sdk-card">
      <h3>{name}</h3>
      <p>{description}</p>
      <code>{install}</code>
    </Link>
  );
}

export default function Home(): React.JSX.Element {
  const {siteConfig} = useDocusaurusContext();

  return (
    <Layout title="Home" description={siteConfig.tagline}>
      <main style={{maxWidth: '860px', margin: '0 auto', padding: '5rem 1.5rem 4rem'}}>

        {/* Hero */}
        <div style={{textAlign: 'center', marginBottom: '3rem'}}>
          <img
            src="/img/icon.svg"
            alt="NobodyWho"
            width={72}
            height={72}
            style={{marginBottom: '1.25rem'}}
          />
          <h1 style={{fontSize: '2.75rem', fontWeight: 700, marginBottom: '0.75rem', letterSpacing: '-0.02em', fontFamily: "'Apfel Grotezk', var(--ifm-font-family-base)", color: 'var(--nw-logo)'}}>
            NobodyWho
          </h1>
          <p style={{
            fontSize: '1.3rem',
            color: 'var(--ifm-font-color-secondary)',
            maxWidth: '560px',
            margin: '0 auto 1.5rem',
            lineHeight: 1.5,
          }}>
            Local-first LLM inference for your apps
          </p>
          <p style={{
            color: 'var(--ifm-font-color-secondary)',
            maxWidth: '620px',
            margin: '0 auto',
            lineHeight: 1.7,
            fontSize: '1rem',
          }}>
            Run open-weight language models directly inside your software.
            Streaming chat, tool calling, structured output, embeddings, text to speech, speech to text and RAG. 
            All offline with GPU acceleration. No servers, no API keys, no
            Docker. Built on{' '}
            <a href="https://github.com/ggml-org/llama.cpp" target="_blank" rel="noreferrer">
              llama.cpp
            </a>.
          </p>
        </div>

        <h2 className="home-section-heading">Get started</h2>
        <div className="get-started-grid">
          <div className="home-card start-card">
            <h3>New to local LLMs?</h3>
            <p>
              Start here if you are new to running language models locally. These
              guides cover the core concepts — what models are, how to pick
              one, and how quantization works.
            </p>
            <div className="start-card-actions">
              <Link to="/docs/llm-basics" className="button button--secondary">
                LLM Basics
              </Link>
              <Link to="/docs/model-selection" className="button button--secondary">
                Model Selection
              </Link>
            </div>
          </div>

          <div className="home-card start-card">
            <h3>Using an AI coding agent?</h3>
            <p>
              Install the NobodyWho skill so your agent can look up the current APIs and documentation.
            </p>
            <div className="skill-install-command">
              <CodeBlock language="bash">{skillInstallCommand}</CodeBlock>
            </div>
          </div>
        </div>

        <h2 className="home-section-heading">Choose your binding</h2>
        <div className="sdk-grid">
          {sdks.map((sdk) => (
            <SDKCard key={sdk.name} {...sdk} />
          ))}
        </div>
      </main>

      <style>{`
        .home-section-heading {
          margin-bottom: 1.25rem;
          font-size: 1.4rem;
          font-weight: 600;
        }

        .get-started-grid {
          display: grid;
          gap: 1rem;
          margin-bottom: 4rem;
        }

        .home-card {
          min-width: 0;
          padding: 1.75rem;
          border: 1px solid var(--ifm-toc-border-color);
          border-radius: 12px;
          background: var(--ifm-background-surface-color);
        }

        .home-card h3 {
          font-size: 1.2rem;
          font-weight: 600;
        }

        .home-card p {
          color: var(--ifm-font-color-secondary);
        }

        .start-card {
          display: flex;
          flex-direction: column;
        }

        .start-card h3 {
          margin: 0 0 0.75rem;
        }

        .start-card p {
          margin-bottom: 1.25rem;
          line-height: 1.7;
        }

        .start-card-actions {
          display: flex;
          flex-wrap: wrap;
          gap: 0.75rem;
          margin-top: auto;
        }

        .sdk-grid {
          display: grid;
          grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
          gap: 1rem;
        }

        .sdk-card {
          display: block;
          color: inherit;
          text-decoration: none;
          transition: border-color 0.2s, transform 0.15s;
        }

        .sdk-card h3 {
          margin: 0 0 0.5rem;
        }

        .sdk-card p {
          margin: 0 0 0.75rem;
          font-size: 0.9rem;
          line-height: 1.5;
        }

        .sdk-card code {
          color: var(--ifm-font-color-secondary);
          font-size: 0.8rem;
        }

        .sdk-card:hover {
          border-color: var(--ifm-color-primary) !important;
          transform: translateY(-2px);
        }

        .skill-install-command {
          min-width: 0;
          margin-top: auto;
        }

        .skill-install-command pre {
          margin: 0;
          padding-right: 3.25rem;
        }

        .skill-install-command button[aria-label='Toggle word wrap'] {
          display: none;
        }

        .skill-install-command button[aria-label='Copy code to clipboard'] {
          border: 1px solid var(--ifm-toc-border-color);
          border-radius: 6px;
          background: var(--ifm-background-surface-color);
          opacity: 1;
        }

      `}</style>
    </Layout>
  );
}
