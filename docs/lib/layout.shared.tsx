import type { BaseLayoutProps } from 'fumadocs-ui/layouts/shared';
import { SiteBrand } from '@/components/site-brand';

export function baseOptions(): BaseLayoutProps {
  return {
    nav: {
      title: <SiteBrand />,
    },
    githubUrl: 'https://github.com/BoundaryML/baml',
    links: [
      { text: 'BAML', url: '/baml' },
      { text: 'CLI', url: '/cli' },
      { text: 'BWS', url: '/bws', description: 'Boundary Web Services' },
      { text: 'Tutorials', url: '/tutorials' },
      { text: 'Examples', url: '/examples' },
    ],
  };
}
