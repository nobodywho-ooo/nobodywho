import {themes as prismThemes} from 'prism-react-renderer';
import type {Config} from '@docusaurus/types';
import type * as Preset from '@docusaurus/preset-classic';

// Per-binding version labels (match the latest released version)
const bindingVersions: Record<string, string> = {
  python: '1.4.0',
  swift: '2.0.0',
  'react-native': '2.2.0',
  flutter: '2.2.0',
  godot: '9.3.0',
};

function sdkDocsConfig(id: string) {
  return {
    lastVersion: 'current',
    versions: {
      current: {label: bindingVersions[id] ?? 'Latest', path: ''},
    },
  };
}

const config: Config = {
  clientModules: ['./src/github-stars.js'],

  title: 'NobodyWho',
  tagline: 'Local-first LLM inference for Swift, Python, Godot, Flutter, and React Native',
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
        docsRouteBasePath: ['docs', 'python', 'swift', 'react-native', 'flutter', 'godot'],
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
        {to: '/python/', label: 'Python', position: 'left', activeBaseRegex: '/python/'},
        {to: '/swift/', label: 'Swift', position: 'left', activeBaseRegex: '/swift/'},
        {to: '/react-native/', label: 'React Native', position: 'left', activeBaseRegex: '/react-native/'},
        {to: '/flutter/', label: 'Flutter', position: 'left', activeBaseRegex: '/flutter/'},
        {to: '/godot/install', label: 'Godot', position: 'left', activeBaseRegex: '/godot/'},
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
      style: 'dark',
      copyright: 'NobodyWho — EUPL-1.2 — <a href="https://docs.nobodywho.ooo/llms.txt">llms.txt</a> · <a href="https://docs.nobodywho.ooo/llms-full.txt">llms-full.txt</a>',
    },
    prism: {
      theme: prismThemes.github,
      darkTheme: prismThemes.dracula,
      additionalLanguages: ['bash', 'dart', 'swift', 'json', 'toml'],
    },
  } satisfies Preset.ThemeConfig,
};

export default config;
