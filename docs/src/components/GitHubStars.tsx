import React, {useState, useEffect} from 'react';

const REPO = 'nobodywho-ooo/nobodywho';
const CACHE_KEY = 'gh-stars-nobodywho';
const CACHE_TTL = 1000 * 60 * 30; // 30 minutes

function formatCount(n: number): string {
  if (n >= 1000) return (n / 1000).toFixed(1).replace(/\.0$/, '') + 'k';
  return String(n);
}

export default function GitHubStars(): React.JSX.Element {
  const [stars, setStars] = useState<number | null>(null);

  useEffect(() => {
    // Check cache first
    try {
      const cached = localStorage.getItem(CACHE_KEY);
      if (cached) {
        const {count, ts} = JSON.parse(cached);
        if (Date.now() - ts < CACHE_TTL) {
          setStars(count);
          return;
        }
      }
    } catch {}

    fetch(`https://api.github.com/repos/${REPO}`)
      .then((r) => r.json())
      .then((data) => {
        if (data.stargazers_count != null) {
          setStars(data.stargazers_count);
          try {
            localStorage.setItem(
              CACHE_KEY,
              JSON.stringify({count: data.stargazers_count, ts: Date.now()}),
            );
          } catch {}
        }
      })
      .catch(() => {});
  }, []);

  return (
    <a
      href={`https://github.com/${REPO}`}
      target="_blank"
      rel="noreferrer noopener"
      className="header-github-link navbar__item"
      aria-label="GitHub repository"
      style={{
        display: 'flex',
        alignItems: 'center',
        gap: '6px',
        textDecoration: 'none',
        color: 'var(--ifm-font-color-secondary)',
        fontSize: '0.85rem',
        padding: '0.25rem 0',
      }}
    >
      <span className="github-icon" />
      {stars !== null && (
        <span style={{fontWeight: 400}}>{formatCount(stars)}</span>
      )}
    </a>
  );
}
