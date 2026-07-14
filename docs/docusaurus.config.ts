import {themes as prismThemes} from 'prism-react-renderer';
import type {Config} from '@docusaurus/types';
import type * as Preset from '@docusaurus/preset-classic';

// The latest tagged release per binding. This is the default version users see
// at /<binding>/. Bumping this requires a matching snapshot in
// `<binding>_versioned_docs/` (see docs/README.md).
const latestReleases: Record<string, string> = {
  kotlin: '2.1.0',
  python: '1.6.0',
  swift: '2.2.0',
  'react-native': '2.4.0',
  flutter: '2.4.0',
  godot: '9.5.0',
};

// `current` reflects the `main` branch — possibly ahead of the latest tag.
// It's published at /<binding>/main/ and gets the "unreleased" banner
// automatically because it's newer than `lastVersion`. Older snapshotted
// versions get the "unmaintained" banner automatically.
function sdkDocsConfig(id: string) {
  return {
    lastVersion: latestReleases[id],
    versions: {
      current: {label: 'main', path: 'main'},
    },
  };
}

const config: Config = {
  clientModules: ['./src/github-stars.js'],

  title: 'NobodyWho',
  tagline: 'Local-first LLM inference for Kotlin, Swift, Python, Godot, Flutter, and React Native',
  favicon: 'img/favicon.ico',

  url: 'https://docs.nobodywho.ooo',
  baseUrl: '/',

  organizationName: 'nobodywho-ooo',
  projectName: 'nobodywho',

  onBrokenLinks: 'throw',

  markdown: {
    hooks: {
      onBrokenMarkdownImages: 'throw',
      onBrokenMarkdownLinks: 'throw',
    },
  },

  scripts: [
    {
      src: 'https://plausible.io/js/pa-AqBGVqlDgFry_9WZW3j-D.js',
      async: true,
      defer: true,
    },
  ],

  headTags: [
    {
      tagName: 'script',
      attributes: {},
      innerHTML: 'window.plausible = window.plausible || function() { (plausible.q = plausible.q || []).push(arguments) }; plausible.init = plausible.init || function(i) { plausible.o = i || {} }; plausible.init();',
    },
  ],

  i18n: {
    defaultLocale: 'en',
    locales: ['en'],
  },

  presets: [
    [
      'classic',
      {
        // The default docs instance holds shared content (LLM Basics, Model Selection)
        docs: {
          path: 'docs',
          routeBasePath: 'docs',
          sidebarPath: './sidebars/shared.ts',
        },
        blog: false,
        theme: {
          customCss: './src/css/custom.css',
        },
      } satisfies Preset.Options,
    ],
  ],

  themes: [
    [
      '@easyops-cn/docusaurus-search-local',
      {
        hashed: true,
        indexBlog: false,
        docsRouteBasePath: ['docs', 'kotlin', 'python', 'swift', 'react-native', 'flutter', 'godot'],
      },
    ],
  ],

  plugins: [
    // LLM-friendly output (llms.txt and llms-full.txt)
    './plugins/llms-txt/index.js',
    // ---- Per-binding docs instances (independently versioned) ----
    [
      '@docusaurus/plugin-content-docs',
      {
        id: 'kotlin',
        path: 'docs-kotlin',
        routeBasePath: 'kotlin',
        sidebarPath: './sidebars/kotlin.ts',
        ...sdkDocsConfig('kotlin'),
      },
    ],
    [
      '@docusaurus/plugin-content-docs',
      {
        id: 'python',
        path: 'docs-python',
        routeBasePath: 'python',
        sidebarPath: './sidebars/python.ts',
        ...sdkDocsConfig('python'),
      },
    ],
    [
      '@docusaurus/plugin-content-docs',
      {
        id: 'swift',
        path: 'docs-swift',
        routeBasePath: 'swift',
        sidebarPath: './sidebars/swift.ts',
        ...sdkDocsConfig('swift'),
      },
    ],
    [
      '@docusaurus/plugin-content-docs',
      {
        id: 'react-native',
        path: 'docs-react-native',
        routeBasePath: 'react-native',
        sidebarPath: './sidebars/react-native.ts',
        ...sdkDocsConfig('react-native'),
      },
    ],
    [
      '@docusaurus/plugin-content-docs',
      {
        id: 'flutter',
        path: 'docs-flutter',
        routeBasePath: 'flutter',
        sidebarPath: './sidebars/flutter.ts',
        ...sdkDocsConfig('flutter'),
      },
    ],
    [
      '@docusaurus/plugin-content-docs',
      {
        id: 'godot',
        path: 'docs-godot',
        routeBasePath: 'godot',
        sidebarPath: './sidebars/godot.ts',
        ...sdkDocsConfig('godot'),
      },
    ],
  ],

  themeConfig: {
    colorMode: {
      defaultMode: 'dark',
      respectPrefersColorScheme: true,
    },
    navbar: {
      title: 'NobodyWho',
      logo: {
        alt: 'NobodyWho',
        src: 'img/icon.svg',
      },
      items: [
        // Basics
        {
          type: 'docSidebar',
          sidebarId: 'shared',
          position: 'left',
          label: 'Basics',
        },
        // Per-binding links
        {to: '/kotlin/', label: 'Kotlin', position: 'left', activeBaseRegex: '/kotlin/'},
        {to: '/python/', label: 'Python', position: 'left', activeBaseRegex: '/python/'},
        {to: '/swift/', label: 'Swift', position: 'left', activeBaseRegex: '/swift/'},
        {to: '/react-native/', label: 'React Native', position: 'left', activeBaseRegex: '/react-native/'},
        {to: '/flutter/', label: 'Flutter', position: 'left', activeBaseRegex: '/flutter/'},
        {to: '/godot/', label: 'Godot', position: 'left', activeBaseRegex: '/godot/'},
        // Right side
        {
          href: 'https://github.com/nobodywho-ooo/nobodywho',
          position: 'right',
          className: 'header-github-link',
          'aria-label': 'GitHub repository',
        },
        {
          href: 'https://discord.gg/qhaMc2qCYB',
          position: 'right',
          className: 'header-discord-link',
          'aria-label': 'Discord server',
        },
      ],
    },
    footer: {
      copyright: '<a href="https://www.nobodywho.ai/">NobodyWho.ai</a> — EUPL-1.2 — <a href="https://docs.nobodywho.ooo/llms.txt">llms.txt</a> · <a href="https://docs.nobodywho.ooo/llms-full.txt">llms-full.txt</a>',
    },
    prism: {
      theme: prismThemes.github,
      darkTheme: prismThemes.dracula,
      additionalLanguages: ['bash', 'dart', 'kotlin', 'swift', 'json', 'toml'],
    },
  } satisfies Preset.ThemeConfig,
};

export default config;
