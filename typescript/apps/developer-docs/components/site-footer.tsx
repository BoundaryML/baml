import { siteConfig } from "@/lib/config"

export function SiteFooter() {
  return (
    <footer className="group-has-[[data-slot=docs]]/body:hidden dark:bg-transparent 3xl:fixed:bg-transparent">
      <div className="container-wrapper px-4 xl:px-6">
        <div className="flex h-(--footer-height) items-center justify-between">
          <div className="w-full px-1 text-center text-xs leading-loose text-muted-foreground sm:text-sm">
            Built by <a href="https://boundaryml.com" target="_blank" rel="noreferrer" className="font-medium underline underline-offset-4">Boundary</a>. The source code is available on <a href={siteConfig.github} target="_blank" rel="noreferrer" className="font-medium underline underline-offset-4">GitHub</a>.
          </div>
        </div>
      </div>
    </footer>
  )
}
