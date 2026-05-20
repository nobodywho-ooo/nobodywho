import React from 'react';
import Layout from '@theme/Layout';
import Link from '@docusaurus/Link';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';

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
    name: 'Flutter',
    description: 'Cross-platform plugin for Flutter on mobile and desktop.',
    install: 'flutter pub add nobodywho',
    link: '/flutter/',
  },
  {
    name: 'Godot',
    description: 'GDExtension for Godot 4.x game projects.',
    install: 'Asset Library or GitHub release',
    link: '/godot/install',
  },
];

function SDKCard({name, description, install, link}: {
  name: string;
  description: string;
  install: string;
  link: string;
}) {
  return (
    <Link
      to={link}
      className="sdk-card"
      style={{
        display: 'block',
        padding: '1.75rem',
        borderRadius: '12px',
        border: '1px solid var(--ifm-toc-border-color)',
        backgroundColor: 'var(--ifm-background-surface-color)',
        textDecoration: 'none',
        color: 'inherit',
        transition: 'border-color 0.2s, transform 0.15s',
      }}
    >
      <h3 style={{margin: '0 0 0.5rem', fontSize: '1.2rem', fontWeight: 600}}>{name}</h3>
      <p style={{margin: '0 0 0.75rem', color: 'var(--ifm-font-color-secondary)', fontSize: '0.9rem', lineHeight: 1.5}}>
        {description}
      </p>
      <code style={{fontSize: '0.8rem', color: 'var(--ifm-font-color-secondary)'}}>{install}</code>
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
          <h1 style={{fontSize: '2.75rem', fontWeight: 700, marginBottom: '0.75rem', letterSpacing: '-0.02em', fontFamily: "'Apfel Grotezk', var(--ifm-font-family-base)"}}>
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
            Streaming chat, tool calling, structured output, embeddings, and RAG
            — all offline with GPU acceleration. No servers, no API keys, no
            Docker. Built on{' '}
            <a href="https://github.com/ggml-org/llama.cpp" target="_blank" rel="noreferrer">
              llama.cpp
            </a>.
          </p>
        </div>

        {/* Basics — first section */}
        <div style={{
          marginBottom: '4rem',
          padding: '2rem',
          borderRadius: '12px',
          border: '1px solid var(--ifm-toc-border-color)',
          backgroundColor: 'var(--ifm-background-surface-color)',
        }}>
          <h2 style={{fontSize: '1.3rem', fontWeight: 600, marginTop: 0, marginBottom: '0.75rem'}}>
            New to local LLMs?
          </h2>
          <p style={{
            color: 'var(--ifm-font-color-secondary)',
            marginBottom: '1.25rem',
            lineHeight: 1.7,
          }}>
            Start here if you are new to running language models locally. These
            guides cover the core concepts — what models are, how to pick
            one, and how quantization works.
          </p>
          <div style={{display: 'flex', gap: '0.75rem', flexWrap: 'wrap'}}>
            <Link to="/docs/llm-basics" className="button button--secondary">
              LLM Basics
            </Link>
            <Link to="/docs/model-selection" className="button button--secondary">
              Model Selection
            </Link>
          </div>
        </div>

        {/* SDK cards */}
        <h2 style={{fontSize: '1.4rem', fontWeight: 600, marginBottom: '1.25rem'}}>
          Choose your binding
        </h2>
        <div style={{
          display: 'grid',
          gridTemplateColumns: 'repeat(auto-fill, minmax(240px, 1fr))',
          gap: '1rem',
        }}>
          {sdks.map((sdk) => (
            <SDKCard key={sdk.name} {...sdk} />
          ))}
        </div>
      </main>

      <style>{`
        .sdk-card:hover {
          border-color: var(--ifm-color-primary) !important;
          transform: translateY(-2px);
        }
      `}</style>
    </Layout>
  );
}
