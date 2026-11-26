import type {Config} from '@docusaurus/types';
import type * as Preset from '@docusaurus/preset-classic';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {withProductSite} from '@beyond10x/docs-system/product-site';

// The documentation pages are the Markdown files of docs/, read in place from the repository
// root. Nothing is copied into website/. The table below gives each file its route, its title and
// the lede the docs-system theme draws over it. The landing page is src/pages/index.tsx, which
// draws product.json with the docs-system product template. design/ and research/ hold internal
// notes and are not published.

const siteDir = path.dirname(fileURLToPath(import.meta.url));
const repositoryRoot = path.resolve(siteDir, "..");
const sourceBase = "https://github.com/beyond10x/shellspec/blob/main/";

interface Page {
  slug: string;
  title: string;
  sidebar_label?: string;
  lede: string;
}

const pages: Record<string, Page> = {
  "docs/index.md": {
    slug: "/",
    title: "shellspec",
    sidebar_label: "Overview",
    lede: "Lift the shell command strings an agent sends into one tree, and give every simple command in them a category.",
  },
  "docs/getting-started.md": {
    slug: "/getting-started",
    title: "Getting started",
    lede: "Build the command-line tool, classify a first command, and call the library.",
  },
  "docs/dialects.md": {
    slug: "/dialects",
    title: "Dialects",
    lede: "bash, sh, zsh and fish lift into the same tree. What each supports, and what becomes unk.",
  },
  "docs/record.md": {
    slug: "/record",
    title: "The record",
    lede: "The tree, the words, the invocation list, the dominant invocation and the redacted shape.",
  },
  "docs/rules.md": {
    slug: "/rules",
    title: "Rules and categories",
    lede: "The 21 categories, how a rule matches, and how to write and check your own rules.",
  },
  "docs/integrating.md": {
    slug: "/integrating",
    title: "Using shellspec from another tool",
    sidebar_label: "Integrating",
    lede: "The library functions, JSON output for other languages, corpus counts and storing records without their values.",
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
          include: ["docs/*.md"],
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
      title: "shellspec",
      items: [
        {to: "/docs/", label: "Docs", position: "left", activeBaseRegex: "^/shellspec/docs"},
        {href: "https://github.com/beyond10x/shellspec", label: "GitHub", position: "right"},
      ],
    },
    footer: {
      links: [
        {
          title: "Documentation",
          items: [
            {label: "Getting started", to: "/docs/getting-started"},
            {label: "Rules and categories", to: "/docs/rules"},
          ],
        },
        {
          title: "Project",
          items: [
            {label: "Source", href: "https://github.com/beyond10x/shellspec"},
            {label: "Changelog", href: "https://github.com/beyond10x/shellspec/blob/main/CHANGELOG.md"},
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
