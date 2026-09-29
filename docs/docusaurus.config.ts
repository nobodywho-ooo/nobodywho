import type {Config} from '@docusaurus/types';
import type * as Preset from '@docusaurus/preset-classic';
import {nobodywhoDark, nobodywhoLight} from './src/prismTheme';

// The latest tagged release per binding. This is the default version users see
// at /<binding>/. Bumping this requires a matching snapshot in
// `<binding>_versioned_docs/` (see docs/README.md).
const latestReleases: Record<string, string> = {
  kotlin: '4.0.0',
  python: '3.0.0',
  swift: '4.0.0',
  'react-native': '4.0.0',
  flutter: '4.0.0',
  godot: '11.0.0',
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
  title: 'NobodyWho',
  tagline: 'Local-first LLM inference for Kotlin, Swift, Python, Flutter, React Native, Expo and Godot',
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
          // Design system tokens first, then the docs theme that uses them
          customCss: [
            './src/css/nobodywho/colors.css',
            './src/css/nobodywho/typography.css',
            './src/css/nobodywho/spacing.css',
            './src/css/nobodywho/effects.css',
            './src/css/custom.css',
          ],
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
    // Follows the reader's system setting (dark if none), with a switch between dark and the light paper theme
    colorMode: {
      defaultMode: 'dark',
      disableSwitch: false,
      respectPrefersColorScheme: true,
    },
    navbar: {
      title: 'nobodywho',
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
        {to: '/react-native/', label: 'RN/Expo', position: 'left', activeBaseRegex: '/react-native/'},
        {to: '/flutter/', label: 'Flutter', position: 'left', activeBaseRegex: '/flutter/'},
        {to: '/godot/', label: 'Godot', position: 'left', activeBaseRegex: '/godot/'},
        // Right side
        {
          href: 'https://github.com/nobodywho-ooo/nobodywho',
          label: 'GitHub',
          position: 'right',
          className: 'header-github-link',
          'aria-label': 'NobodyWho on GitHub',
        },
      ],
    },
    prism: {
      theme: nobodywhoLight,
      darkTheme: nobodywhoDark,
      additionalLanguages: ['bash', 'dart', 'kotlin', 'swift', 'json', 'toml'],
    },
  } satisfies Preset.ThemeConfig,
};

export default config;
