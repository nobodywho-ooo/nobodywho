const fs = require('fs');
const path = require('path');
const matter = require('gray-matter');

const SITE_URL = 'https://docs.nobodywho.ooo';
const DESCRIPTION =
  'Local-first LLM inference for Swift, Python, Godot, Flutter, and React Native, built on llama.cpp. Streaming chat, tool calling, embeddings, and RAG with offline GPU-accelerated inference.';

// Map of docs directories to section names and route prefixes
const SECTIONS = [
  {dir: 'docs', label: 'Home', routeBase: '/docs'},
  {dir: 'docs-python', label: 'Python', routeBase: '/python'},
  {dir: 'docs-swift', label: 'Swift', routeBase: '/swift'},
  {dir: 'docs-react-native', label: 'React Native', routeBase: '/react-native'},
  {dir: 'docs-flutter', label: 'Flutter', routeBase: '/flutter'},
  {dir: 'docs-godot', label: 'Godot', routeBase: '/godot'},
];

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

module.exports = function llmsTxtPlugin(context) {
  return {
    name: 'llms-txt',
    async postBuild({outDir}) {
      const siteDir = context.siteDir;
      const lines = [];
      const fullLines = [];

      lines.push('# NobodyWho');
      lines.push('');
      lines.push(`> ${DESCRIPTION}`);
      lines.push('');
      lines.push(
        `This file is an index of documentation pages. For all content in a single file, see [NobodyWho full docs](${SITE_URL}/llms-full.txt).`,
      );

      for (const section of SECTIONS) {
        const dirPath = path.join(siteDir, section.dir);
        const files = getMarkdownFiles(dirPath);
        if (files.length === 0) continue;

        lines.push('');
        lines.push(`## ${section.label}`);
        lines.push('');

        for (const filePath of files.sort()) {
          const raw = fs.readFileSync(filePath, 'utf-8');
          const {data: frontmatter, content} = matter(raw);
          const title = getTitle(filePath, content, frontmatter);
          const url = fileToUrl(filePath, dirPath, section.routeBase);
          const mdUrl = `${SITE_URL}${url}`;

          lines.push(`- [${title}](${mdUrl})`);
          fullLines.push(`# ${title}\n\n${content}\n\n---\n`);
        }
      }

      const llmsTxt = lines.join('\n') + '\n';
      const llmsFullTxt = fullLines.join('\n');

      fs.writeFileSync(path.join(outDir, 'llms.txt'), llmsTxt);
      fs.writeFileSync(path.join(outDir, 'llms-full.txt'), llmsFullTxt);
    },
  };
};
