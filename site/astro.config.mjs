// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import starlightLinksValidator from 'starlight-links-validator';

const repo = 'https://github.com/mischa-dev/dogubako';

// https://astro.build/config
export default defineConfig({
  site: 'https://docs.dogubako.app',
  trailingSlash: 'ignore',
  integrations: [
    starlight({
      title: 'Dogubako',
      description:
        'Documentation for Dogubako, a local-first, open-source utility toolbox: desktop app, self-hosted web service, CLI, HTTP API and MCP server.',
      logo: {
        light: './src/assets/logo-light.svg',
        dark: './src/assets/logo-dark.svg',
        alt: 'Dogubako',
      },
      favicon: '/favicon.svg',
      head: [
        { tag: 'link', attrs: { rel: 'apple-touch-icon', href: '/apple-touch-icon.png' } },
        { tag: 'meta', attrs: { name: 'theme-color', content: '#f7f6f3', media: '(prefers-color-scheme: light)' } },
        { tag: 'meta', attrs: { name: 'theme-color', content: '#151412', media: '(prefers-color-scheme: dark)' } },
        { tag: 'meta', attrs: { property: 'og:image', content: 'https://docs.dogubako.app/og.png' } },
      ],
      social: [{ icon: 'github', label: 'GitHub', href: repo }],
      editLink: { baseUrl: `${repo}/edit/main/site/` },
      lastUpdated: true,
      customCss: [
        '@fontsource-variable/instrument-sans',
        '@fontsource/dm-mono/400.css',
        '@fontsource/dm-mono/500.css',
        './src/styles/theme.css',
      ],
      plugins: [starlightLinksValidator({ errorOnLocalLinks: false })],
      sidebar: [
        {
          label: 'Start here',
          items: [
            'start/introduction',
            'start/install',
            'start/quick-start',
            'start/concepts',
          ],
        },
        {
          label: 'Using the app',
          items: [
            'app/tour',
            'app/home-and-bench',
            'app/opening-files',
            'app/tool-page',
            'app/results',
            'app/customize',
            'app/keyboard-shortcuts',
            'app/desktop',
            'app/engines-and-models',
          ],
        },
        {
          label: 'Command line',
          items: ['cli/overview', 'cli/recipes', 'cli/scripting'],
        },
        {
          label: 'Self-hosting',
          items: [
            'self-hosting/overview',
            'self-hosting/docker',
            'self-hosting/native',
            'self-hosting/networking',
            'self-hosting/accounts-and-agents',
            'self-hosting/configuration',
            'self-hosting/operations',
          ],
        },
        {
          label: 'AI agents (MCP)',
          items: [
            'agents/overview',
            'agents/local',
            'agents/remote',
            'agents/discovery-and-results',
          ],
        },
        {
          label: 'Tool catalog',
          collapsed: true,
          items: [{ autogenerate: { directory: 'tools' } }],
        },
        {
          label: 'Reference',
          collapsed: true,
          items: [
            'reference/cli',
            'reference/http-api',
            'reference/mcp',
            'reference/configuration',
            'reference/errors',
            'reference/permissions-and-policies',
            'reference/capabilities',
          ],
        },
        {
          label: 'Extending',
          collapsed: true,
          items: [
            'develop/architecture',
            'develop/building',
            'develop/writing-tools',
            'develop/extensions',
            'develop/frontend',
            'develop/contributing',
            'develop/docs-site',
          ],
        },
        {
          label: 'Project',
          collapsed: true,
          items: [
            'project/security',
            'project/licensing',
            'project/performance',
            'project/troubleshooting',
            'project/faq',
          ],
        },
      ],
    }),
  ],
});
