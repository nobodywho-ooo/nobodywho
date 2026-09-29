import React, {useState, useRef, useEffect} from 'react';
import useIsBrowser from '@docusaurus/useIsBrowser';

const GH_BASE =
  'https://github.com/nobodywho-ooo/nobodywho/blob/main/docs/';
const RAW_BASE =
  'https://raw.githubusercontent.com/nobodywho-ooo/nobodywho/main/docs/';

function deriveSourcePath(pathname: string): string {
  const clean = pathname.replace(/\/$/, '') || '/docs';
  const prefixMap: Record<string, string> = {
    '/python': 'docs-python',
    '/swift': 'docs-swift',
    '/react-native': 'docs-react-native',
    '/flutter': 'docs-flutter',
    '/godot': 'docs-godot',
    '/docs': 'docs',
  };
  for (const [prefix, dir] of Object.entries(prefixMap)) {
    if (clean === prefix || clean.startsWith(prefix + '/')) {
      const slug = clean.slice(prefix.length).replace(/^\//, '') || 'index';
      return `${dir}/${slug}.md`;
    }
  }
  return `docs${clean}.md`;
}

function CopyPageButton() {
  const [copied, setCopied] = useState(false);

  async function handleCopy() {
    try {
      const article = document.querySelector('article');
      if (article) {
        await navigator.clipboard.writeText(article.innerText);
        setCopied(true);
        setTimeout(() => setCopied(false), 1500);
      }
    } catch {}
  }

  return (
    <button type="button" onClick={handleCopy} className="page-action">
      {copied ? 'Copied' : 'Copy page'}
    </button>
  );
}

export default function PageActions(): React.JSX.Element | null {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  // False during SSR and hydration, so the first client render matches the server HTML
  const isBrowser = useIsBrowser();

  useEffect(() => {
    function handleClick(e: MouseEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    }
    function handleKey(e: KeyboardEvent) {
      if (e.key === 'Escape') setOpen(false);
    }
    document.addEventListener('click', handleClick);
    document.addEventListener('keydown', handleKey);
    return () => {
      document.removeEventListener('click', handleClick);
      document.removeEventListener('keydown', handleKey);
    };
  }, []);

  if (!isBrowser) return null;

  const pathname = window.location.pathname;
  const sourcePath = deriveSourcePath(pathname);
  const pageUrl = window.location.href;
  const rawMdUrl = `${RAW_BASE}${sourcePath}`;
  const githubUrl = `${GH_BASE}${sourcePath}`;
  const q = encodeURIComponent(`Read ${pageUrl}, I want to ask questions about it.`);

  const items = [
    {label: 'ChatGPT', href: `https://chatgpt.com/?hints=search&q=${q}`},
    {label: 'Claude', href: `https://claude.ai/new?q=${q}`},
    {label: 'View raw .md', href: rawMdUrl},
    {label: 'View on GitHub', href: githubUrl},
  ];

  return (
    <div className="page-actions">
      <CopyPageButton />
      <div ref={ref} className="page-actions__menu">
        <button
          type="button"
          onClick={() => setOpen(!open)}
          className="page-action page-action--menu"
          aria-haspopup="menu"
          aria-expanded={open}
        >
          Open in
        </button>
        {open && (
          <div role="menu" className="page-actions__list">
            {items.map(({label, href}) => (
              <a key={label} href={href} target="_blank" rel="noreferrer noopener" role="menuitem">
                {label}
              </a>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
