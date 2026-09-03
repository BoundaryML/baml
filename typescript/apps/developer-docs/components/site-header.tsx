import Link from "next/link"
import { ExternalLink } from "lucide-react"

import { siteConfig } from "@/lib/config"
import { CommandMenu } from "@/components/command-menu"
import { GitHubLink } from "@/components/github-link"
import { MainNav } from "@/components/main-nav"
import { MobileNav } from "@/components/mobile-nav"
import { ModeSwitcher } from "@/components/mode-switcher"
import { Button } from "@/components/ui/button"
import { Separator } from "@/components/ui/separator"

export function SiteHeader() {
  return (
    <header className="sticky top-0 z-50 w-full bg-background">
      <div className="container-wrapper px-6 3xl:fixed:px-0">
        <div className="flex h-(--header-height) items-center **:data-[slot=separator]:h-4! 3xl:fixed:container">
          <MobileNav items={siteConfig.navItems} className="flex lg:hidden" />
          <MainNav items={siteConfig.navItems} className="hidden lg:flex" />
          <div className="ml-auto flex items-center gap-2 md:flex-1 md:justify-end">
            <div className="hidden w-full flex-1 md:flex md:w-auto md:flex-none"><CommandMenu /></div>
            <Separator orientation="vertical" className="ml-2 hidden lg:block" />
            <GitHubLink />
            <Separator orientation="vertical" />
            <ModeSwitcher />
            <div className="flex items-center gap-2">
              <Separator orientation="vertical" />
              <Button asChild size="sm" className="h-[31px] rounded-lg">
                <Link href={siteConfig.playground} target="_blank" rel="noreferrer"><ExternalLink />Playground</Link>
              </Button>
            </div>
          </div>
        </div>
      </div>
    </header>
  )
}
