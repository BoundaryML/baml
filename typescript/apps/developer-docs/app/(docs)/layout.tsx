import { source } from "@/lib/source"
import { DocsSidebar } from "@/components/docs-sidebar"
import { SiteFooter } from "@/components/site-footer"
import { SiteHeader } from "@/components/site-header"
import { SidebarProvider } from "@/components/ui/sidebar"

export default function DocsLayout({ children }: { children: React.ReactNode }) {
  return (
    <div data-slot="layout" className="group/layout relative z-10 flex min-h-svh flex-col bg-background">
      <SiteHeader />
      <main className="flex min-h-0 flex-1 flex-col">
        <div className="container-wrapper flex flex-1 flex-col px-2">
          <SidebarProvider className="min-h-min flex-1 items-start px-0 [--top-spacing:0] lg:grid lg:grid-cols-[var(--sidebar-width)_minmax(0,1fr)] lg:[--top-spacing:calc(var(--spacing)*4)] 3xl:fixed:container 3xl:fixed:px-3" style={{ "--sidebar-width": "calc(var(--spacing) * 72)" } as React.CSSProperties}>
            <DocsSidebar tree={source.pageTree} />
            <div className="h-full w-full">{children}</div>
          </SidebarProvider>
        </div>
      </main>
      <SiteFooter />
    </div>
  )
}
