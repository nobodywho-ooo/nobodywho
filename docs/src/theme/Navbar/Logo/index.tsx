import React from 'react';
import Link from '@docusaurus/Link';

// "nobodywho | DOCS" wordmark: one link to the docs home. Hovering anywhere on it lights up DOCS.
export default function NavbarLogo(): React.JSX.Element {
  return (
    <Link className="navbar__brand nw-brand" to="/" aria-label="nobodywho docs home">
      <span className="navbar__title nw-brand__name">nobodywho</span>
      <span className="nw-brand__docs">Docs</span>
    </Link>
  );
}
