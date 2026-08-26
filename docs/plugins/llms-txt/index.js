const fs = require('fs');
const path = require('path');
const matter = require('gray-matter');

const SITE_URL = 'https://docs.nobodywho.ooo';
const DESCRIPTION =
  'NobodyWho runs LLMs locally in Kotlin, Swift, Python, Godot, Flutter, and React Native. It uses llama.cpp and supports streaming chat, tool calling, embeddings, RAG, offline inference, and GPU acceleration.';

// Map of documentation sources to section names and route prefixes
const SECTIONS = [
  {dir: 'docs', label: 'Home', routeBase: '/docs'},
  {id: 'kotlin', label: 'Kotlin', routeBase: '/kotlin'},
  {id: 'python', label: 'Python', routeBase: '/python'},
  {id: 'swift', label: 'Swift', routeBase: '/swift'},
  {id: 'react-native', label: 'React Native', routeBase: '/react-native'},
  {id: 'flutter', label: 'Flutter', routeBase: '/flutter'},
  {id: 'godot', label: 'Godot', routeBase: '/godot'},
];

function getDocsSource({section, siteDir}) {
  if (section.dir) return {dirPath: path.join(siteDir, section.dir), version: null};

  const versionsPath = path.join(siteDir, `${section.id}_versions.json`);
  const [latestVersion] = JSON.parse(fs.readFileSync(versionsPath, 'utf-8'));
  if (!latestVersion) throw new Error(`No released version found for ${section.id}`);

  return {
    dirPath: path.join(siteDir, `${section.id}_versioned_docs`, `version-${latestVersion}`),
    version: latestVersion,
  };
}

function getMarkdownFiles(dirPath) {
  if (!fs.existsSync(dirPath)) return [];
  const files = [];
  for (const entry of fs.readdirSync(dirPath, {withFileTypes: true})) {
    const full = path.join(dirPath, entry.name);
    if (entry.isDirectory()) {
      files.push(...getMarkdownFiles(full));
    } else if (entry.name.endsWith('.md') || entry.name.endsWith('.mdx')) {
      files.push(full);
    }
  }
  return files;
}

function fileToUrl(filePath, dirPath, routeBase) {
  let rel = path.relative(dirPath, filePath);
  // Remove extension
  rel = rel.replace(/\.(md|mdx)$/, '');
  // index -> directory
  if (rel === 'index') return routeBase + '/';
  rel = rel.replace(/\/index$/, '/');
  return routeBase + '/' + rel;
}

function getTitle(filePath, content, frontmatter) {
  if (frontmatter.title) return frontmatter.title;
  // Extract first heading
  const match = content.match(/^#\s+(.+)$/m);
  if (match) return match[1];
  return path.basename(filePath, path.extname(filePath));
}

function absoluteDocsUrl({pageUrl, target}) {
  const url = new URL(target, `${SITE_URL}${pageUrl}`);
  url.pathname = url.pathname.replace(/\.mdx?$/, '');
  return url.toString();
}

function normalizeMarkdown({content, pageUrl, title}) {
  const lines = [];
  let inAdmonition = false;
  let inFence = false;
  let seenContent = false;

  for (let line of content.split('\n')) {
    if (/^\s*(```|~~~)/.test(line)) {
      inFence = !inFence;
      lines.push(line);
      seenContent = true;
      continue;
    }
    if (inFence) {
      lines.push(line);
      continue;
    }

    if (/^\s*import\s/.test(line)) continue;

    line = line.replace(
      /^\s*<Link\s+to=(["'])(.*?)\1[^>]*>(.*?)<\/Link>\s*$/,
      (_match, _quote, target, label) => `- [${label}](${target})`,
    );

    if (/^\s*<\/?(?:div|details)[^>]*>\s*$/.test(line)) continue;

    const summary = line.match(/^\s*<summary>(.*?)<\/summary>\s*$/);
    if (summary) line = `**${summary[1]}**`;

    const admonition = line.match(/^\s*:::(info|warning|note|tip|danger)\s*$/);
    if (admonition) {
      const label = admonition[1][0].toUpperCase() + admonition[1].slice(1);
      lines.push(`**${label}:**`);
      inAdmonition = true;
      seenContent = true;
      continue;
    }
    if (inAdmonition && /^\s*:::\s*$/.test(line)) {
      inAdmonition = false;
      continue;
    }

    line = line.replace(
      /(!?\[[^\]]*\]\()([^)\s]+)(\s+(?:"[^"]*"|'[^']*'))?(\))/g,
      (match, prefix, target, suffix = '', closing) => {
        if (/^[a-z][a-z0-9+.-]*:/i.test(target) || target.startsWith('//')) return match;
        return `${prefix}${absoluteDocsUrl({pageUrl, target})}${suffix}${closing}`;
      },
    );

    const heading = line.match(/^(#{1,4})\s+(.+)$/);
    if (heading) {
      if (!seenContent && heading[1] === '#' && heading[2].toLowerCase() === title.toLowerCase()) {
        continue;
      }
      line = `${'#'.repeat(heading[1].length + 2)} ${heading[2]}`;
    }

    lines.push(line);
    if (line.trim()) seenContent = true;
  }

  return lines.join('\n').trim();
}

module.exports = function llmsTxtPlugin(context) {
  return {
    name: 'llms-txt',
    async postBuild({outDir}) {
      const siteDir = context.siteDir;
      const lines = [];
      const fullLines = [
        '# NobodyWho',
        '',
        `> ${DESCRIPTION}`,
        '',
        'The binding sections use the latest released documentation. The main branch may document unreleased features. Main branch documentation is not included.',
      ];

      lines.push('# NobodyWho');
      lines.push('');
      lines.push(`> ${DESCRIPTION}`);
      lines.push('');
      lines.push(
        `This file is an index of documentation pages. For all content in a single file, see [NobodyWho full docs](${SITE_URL}/llms-full.txt).`,
      );

      for (const section of SECTIONS) {
        const {dirPath, version} = getDocsSource({section, siteDir});
        const files = getMarkdownFiles(dirPath);
        if (files.length === 0) continue;

        lines.push('');
        lines.push(`## ${section.label}`);
        lines.push('');
        fullLines.push('', `## ${section.label}`, '');
        if (version) fullLines.push(`This section documents ${section.label} ${version}, the latest released version.`, '');

        for (const filePath of files.sort()) {
          const raw = fs.readFileSync(filePath, 'utf-8');
          const {data: frontmatter, content} = matter(raw);
          const title = getTitle(filePath, content, frontmatter);
          const url = fileToUrl(filePath, dirPath, section.routeBase);
          const mdUrl = `${SITE_URL}${url}`;

          const normalizedContent = normalizeMarkdown({content, pageUrl: url, title});

          lines.push(`- [${title}](${mdUrl})`);
          fullLines.push(
            `### ${title}`,
            '',
            `Source: [${mdUrl}](${mdUrl})`,
            '',
            normalizedContent,
            '',
            '---',
            '',
          );
        }
      }

      const llmsTxt = lines.join('\n') + '\n';
      const llmsFullTxt = fullLines.join('\n').trim() + '\n';

      fs.writeFileSync(path.join(outDir, 'llms.txt'), llmsTxt);
      fs.writeFileSync(path.join(outDir, 'llms-full.txt'), llmsFullTxt);
    },
  };
};
