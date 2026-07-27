import {themes as prismThemes} from 'prism-react-renderer';
import type {Config} from '@docusaurus/types';
import type * as Preset from '@docusaurus/preset-classic';

const config: Config = {
  title: 'specforge',
  tagline: 'Forge production-ready SDKs from OpenAPI specs',
  favicon: 'img/favicon.ico',

  future: {
    v4: true,
  },

  url: 'https://specforge.deepwhaleai.com',
  baseUrl: '/docs/',

  organizationName: 'amafjarkasi',
  projectName: 'specforge-openapi-sdk-codegen',

  onBrokenLinks: 'warn',
  onBrokenMarkdownLinks: 'warn',

  i18n: {
    defaultLocale: 'en',
    locales: ['en'],
  },

  presets: [
    [
      'classic',
      {
        docs: {
          sidebarPath: './sidebars.ts',
          editUrl: 'https://github.com/amafjarkasi/specforge-openapi-sdk-codegen/tree/main/',
          routeBasePath: '/',
        },
        blog: false,
        theme: {
          customCss: './src/css/custom.css',
        },
      } satisfies Preset.Options,
    ],
  ],

  themeConfig: {
    colorMode: {
      defaultMode: 'dark',
      disableSwitch: false,
      respectPrefersColorScheme: true,
    },
    navbar: {
      title: 'specforge',
      logo: {
        alt: 'specforge Logo',
        src: 'img/logo.svg',
      },
      items: [
        {
          type: 'docSidebar',
          sidebarId: 'docsSidebar',
          position: 'left',
          label: 'Documentation',
        },
        {
          href: '/',
          label: 'Home',
          position: 'right',
        },
        {
          href: 'https://github.com/amafjarkasi/specforge-openapi-sdk-codegen',
          label: 'GitHub',
          position: 'right',
        },
      ],
    },
    footer: {
      style: 'dark',
      links: [
        {
          title: 'Docs',
          items: [
            {
              label: 'Introduction',
              to: '/',
            },
            {
              label: 'Getting Started',
              to: '/getting-started/release',
            },
            {
              label: 'Plugins',
              to: '/guides/plugins',
            },
          ],
        },
        {
          title: 'Community',
          items: [
            {
              label: 'GitHub',
              href: 'https://github.com/amafjarkasi/specforge-openapi-sdk-codegen',
            },
            {
              label: 'Issues',
              href: 'https://github.com/amafjarkasi/specforge-openapi-sdk-codegen/issues',
            },
          ],
        },
        {
          title: 'More',
          items: [
            {
              label: 'specforge.deepwhaleai.com',
              href: '/',
            },
            {
              label: 'DeepWhale AI',
              href: 'https://deepwhaleai.com',
            },
          ],
        },
      ],
      copyright: `Copyright © ${new Date().getFullYear()} specforge. MIT License. Built with Rust.`,
    },
    prism: {
      theme: prismThemes.github,
      darkTheme: prismThemes.dracula,
      additionalLanguages: ['rust', 'go', 'typescript', 'bash', 'yaml', 'json'],
    },
  } satisfies Preset.ThemeConfig,
};

export default config;
