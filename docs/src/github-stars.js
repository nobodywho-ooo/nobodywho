import ExecutionEnvironment from '@docusaurus/ExecutionEnvironment';

if (ExecutionEnvironment.canUseDOM) {
  const REPO = 'nobodywho-ooo/nobodywho';
  const CACHE_KEY = 'gh-stars-nobodywho';
  const CACHE_TTL = 1000 * 60 * 30;

  function formatCount(n) {
    if (n >= 1000) return (n / 1000).toFixed(1).replace(/\.0$/, '') + 'k';
    return String(n);
  }

  function inject(count) {
    const link = document.querySelector('.header-github-link');
    if (!link || link.querySelector('.gh-star-count')) return;
    const span = document.createElement('span');
    span.className = 'gh-star-count';
    span.textContent = formatCount(count);
    link.appendChild(span);
  }

  function load() {
    try {
      const cached = localStorage.getItem(CACHE_KEY);
      if (cached) {
        const {count, ts} = JSON.parse(cached);
        if (Date.now() - ts < CACHE_TTL) {
          inject(count);
          return;
        }
      }
    } catch {}

    fetch(`https://api.github.com/repos/${REPO}`)
      .then((r) => r.json())
      .then((data) => {
        if (data.stargazers_count != null) {
          inject(data.stargazers_count);
          try {
            localStorage.setItem(CACHE_KEY, JSON.stringify({count: data.stargazers_count, ts: Date.now()}));
          } catch {}
        }
      })
      .catch(() => {});
  }

  // Run on initial load and on route changes (SPA navigation)
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', load);
  } else {
    load();
  }
  // Re-inject after SPA navigations since React may re-render the navbar
  const observer = new MutationObserver(() => {
    const link = document.querySelector('.header-github-link');
    if (link && !link.querySelector('.gh-star-count')) load();
  });
  observer.observe(document.body, {childList: true, subtree: true});
}
