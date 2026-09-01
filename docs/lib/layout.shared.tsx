import type { BaseLayoutProps } from 'fumadocs-ui/layouts/shared';
import { siteName } from './constants';

export function baseOptions(): BaseLayoutProps {
  return {
    nav: {
      title: siteName,
    },
    githubUrl: 'https://github.com/BoundaryML/baml',
    links: [
      { text: 'BAML', url: '/baml' },
      { text: 'CLI', url: '/cli' },
      { text: 'BWS', url: '/bws' },
      { text: 'Tutorials', url: '/tutorials' },
      { text: 'Examples', url: '/examples' },
    ],
  };
}
