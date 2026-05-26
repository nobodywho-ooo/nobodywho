import React from 'react';
import Layout from '@theme-original/DocItem/Layout';
import type LayoutType from '@theme/DocItem/Layout';
import {useDoc} from '@docusaurus/plugin-content-docs/client';
import Link from '@docusaurus/Link';
import PageActions from '@site/src/components/OpenInDropdown';

type Props = React.ComponentProps<typeof LayoutType>;

function TopNavigation() {
  const {previous, next} = useDoc().metadata;

  return (
    <nav style={{
      display: 'flex',
      justifyContent: 'space-between',
      alignItems: 'center',
      marginBottom: '1rem',
      fontSize: '0.85rem',
      flexWrap: 'wrap',
      gap: '0.5rem',
    }}>
      <div style={{display: 'flex', alignItems: 'center', gap: '1rem'}}>
        {previous && (
          <Link to={previous.permalink} className="top-doc-nav-link" style={{
            display: 'inline-flex',
            alignItems: 'center',
            gap: '4px',
            color: 'var(--ifm-font-color-secondary)',
            textDecoration: 'none',
          }}>
            <span aria-hidden="true">&larr;</span> {previous.title}
          </Link>
        )}
        {previous && next && (
          <span style={{color: 'var(--ifm-toc-border-color)'}}>|</span>
        )}
        {next && (
          <Link to={next.permalink} className="top-doc-nav-link" style={{
            display: 'inline-flex',
            alignItems: 'center',
            gap: '4px',
            color: 'var(--ifm-font-color-secondary)',
            textDecoration: 'none',
          }}>
            {next.title} <span aria-hidden="true">&rarr;</span>
          </Link>
        )}
      </div>
      <PageActions />
    </nav>
  );
}

export default function LayoutWrapper(props: Props): React.JSX.Element {
  return (
    <>
      <TopNavigation />
      <Layout {...props} />
    </>
  );
}
