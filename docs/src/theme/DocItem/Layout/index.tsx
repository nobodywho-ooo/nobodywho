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
    <nav className="top-doc-nav" aria-label="Previous and next page">
      <div className="top-doc-nav__links">
        {previous && (
          <Link to={previous.permalink} className="top-doc-nav-link top-doc-nav-link--prev">
            {previous.title}
          </Link>
        )}
        {next && (
          <Link to={next.permalink} className="top-doc-nav-link top-doc-nav-link--next">
            {next.title}
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
