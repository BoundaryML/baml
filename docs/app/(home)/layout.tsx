import { SiteHeader } from '@/components/site-header';

export default function Layout({ children }: LayoutProps<'/'>) {
  return (
    <div className="flex min-h-screen flex-col">
      <SiteHeader />
      {children}
    </div>
  );
}
