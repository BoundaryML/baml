import { source } from '@/lib/source';
import { baseOptions } from '@/lib/layout.shared';
import { DocsSiteHeader } from '@/components/site-header';
import { DocsLayout } from 'fumadocs-ui/layouts/docs';

export default function Layout({ children }: LayoutProps<'/'>) {
  return (
    <DocsLayout
      tree={source.getPageTree()}
      {...baseOptions()}
      containerProps={{ className: 'shadcn-docs-layout' }}
      slots={{ header: DocsSiteHeader }}
    >
      {children}
    </DocsLayout>
  );
}
