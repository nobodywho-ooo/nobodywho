import React from 'react';

// Footer from the NobodyWho design system (nw-foot): wordmark + tagline, uppercase links, location and licence.
const links = [
  {label: 'nobodywho.ai', href: 'https://www.nobodywho.ai/'},
  {label: 'Blog', href: 'https://www.nobodywho.ai/posts/'},
  {label: 'Discord', href: 'https://discord.gg/qhaMc2qCYB'},
];

export default function Footer(): React.JSX.Element {
  return (
    <footer className="nw-foot">
      <div className="nw-foot__brand">
        <a href="https://www.nobodywho.ai/">nobodywho</a>
        <span>Local AI, built in Europe</span>
      </div>
      <nav aria-label="Footer">
        {links.map((l) => (
          <a key={l.label} href={l.href} target="_blank" rel="noopener">
            {l.label}
          </a>
        ))}
      </nav>
      <div className="nw-foot__meta">
        <span>Copenhagen, Denmark</span>
        <span>Open source under EUPL 1.2</span>
        <a href="/llms.txt">llms.txt</a>
        <a href="/llms-full.txt">llms-full.txt</a>
      </div>
    </footer>
  );
}
