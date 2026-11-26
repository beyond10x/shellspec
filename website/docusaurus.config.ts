import type {Config} from '@docusaurus/types';
import type * as Preset from '@docusaurus/preset-classic';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {withProductSite} from '@beyond10x/docs-system/product-site';

// The documentation pages are the repository's own Markdown, read in place from the repository
// root: docs/design.md and research/. Nothing is copied into website/. The table below gives each
// file its route, its title and the header the docs-system theme draws over it (lede, status,
// source link); the files themselves stay as they are on GitHub. The landing page is
// src/pages/index.tsx, which draws product.json with the docs-system product template.

const siteDir = path.dirname(fileURLToPath(import.meta.url));
const repositoryRoot = path.resolve(siteDir, '..');
const sourceBase = 'https://github.com/beyond10x/shellspec/blob/main/';

const releaseState = 'No release yet: the library and the shellspec binary build from source.';

interface Page {
  slug: string;
  title: string;
  sidebar_label?: string;
  lede: string;
  status?: 'planned';
}

const pages: Record<string, Page> = {
  'docs/design.md': {
    slug: '/',
    title: 'Design',
    lede: `What shellspec lifts, the IR, the parsers, the categories and the ruleset. ${releaseState} The specification in spec/ is authoritative where this page differs.`,
  },
  'research/2026-11-26-corpus.md': {
    slug: '/research/corpus',
    title: 'Corpus: 72 hours of Bash tool commands',
    sidebar_label: 'Corpus',
    lede: 'Counts and generic shapes over 331,797 Bash tool commands of one workstation; no command text.',
  },
};

function pageFor(file: string): Page | undefined {
  return pages[file];
}

const config: Config = {
  title: 'shellspec',
  tagline: 'One IR for the shell commands an agent sends.',

  future: {v4: true},
  url: 'https://beyond10x.github.io',
  baseUrl: '/shellspec/',
  organizationName: 'beyond10x',
  projectName: 'shellspec',
  deploymentBranch: 'gh-pages',
  trailingSlash: false,
  onBrokenLinks: 'throw',
  onBrokenAnchors: 'throw',

  markdown: {
    format: 'detect',
    hooks: {onBrokenMarkdownLinks: 'throw'},
    parseFrontMatter: async (params) => {
      const result = await params.defaultParseFrontMatter(params);
      const file = path.relative(repositoryRoot, params.filePath).split(path.sep).join('/');
      const page = pageFor(file);
      if (!page) return result;
      return {
        ...result,
        frontMatter: {
          ...result.frontMatter,
          ...page,
          source: file,
          source_url: sourceBase + file,
        },
      };
    },
  },
  i18n: {defaultLocale: 'en', locales: ['en']},

  presets: [
    [
      'classic',
      {
        docs: {
          path: '..',
          include: ['docs/design.md', 'research/*.md'],
          sidebarPath: './sidebars.ts',
          routeBasePath: 'docs',
          editUrl: 'https://github.com/beyond10x/shellspec/tree/main/',
        },
        blog: false,
      } satisfies Preset.Options,
    ],
  ],

  themeConfig: {
    navbar: {
      title: 'shellspec',
      items: [
        {to: '/docs/', label: 'Design', position: 'left', activeBaseRegex: '^/shellspec/docs/?$'},
        {to: '/docs/research/corpus', label: 'Research', position: 'left', activeBaseRegex: '^/shellspec/docs/research'},
        {href: 'https://github.com/beyond10x/shellspec', label: 'GitHub', position: 'right'},
      ],
    },
    footer: {
      links: [
        {
          title: 'Documentation',
          items: [
            {label: 'Design', to: '/docs/'},
            {label: 'Corpus', to: '/docs/research/corpus'},
          ],
        },
        {
          title: 'Project',
          items: [
            {label: 'Source', href: 'https://github.com/beyond10x/shellspec'},
            {label: 'README', href: 'https://github.com/beyond10x/shellspec/blob/main/README.md'},
          ],
        },
      ],
      copyright: 'A beyond10x project · Apache-2.0 · built with Docusaurus and the docs-system product template.',
    },
  } satisfies Preset.ThemeConfig,
};

const site = withProductSite(config, {mark: 'S'});

// withProductSite adds its static directories as absolute paths, which the build would ship in its
// bundle; they are made relative to the site so no machine path reaches the built site.
export default {
  ...site,
  staticDirectories: site.staticDirectories?.map((directory) =>
    directory && path.isAbsolute(directory) ? path.relative(siteDir, directory) : directory,
  ),
} satisfies Config;
