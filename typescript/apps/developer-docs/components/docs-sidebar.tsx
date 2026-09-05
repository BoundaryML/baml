"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname } from "next/navigation"

import { siteConfig } from "@/lib/config"
import { DOCS_SIDEBAR_SCROLL_STORAGE_KEY } from "@/lib/docs-sidebar-scroll"
import type { source } from "@/lib/source"
import { Sidebar, SidebarContent, SidebarGroup, SidebarGroupContent, SidebarGroupLabel, SidebarMenu, SidebarMenuButton, SidebarMenuItem } from "@/components/ui/sidebar"

type TreeNode = (typeof source.pageTree.children)[number]

function readScrollState() {
  try { return JSON.parse(sessionStorage.getItem(DOCS_SIDEBAR_SCROLL_STORAGE_KEY) ?? "") as { pathname: string; scrollTop: number } } catch { return null }
}

function saveScrollState(container: HTMLElement) {
  try { sessionStorage.setItem(DOCS_SIDEBAR_SCROLL_STORAGE_KEY, JSON.stringify({ pathname: location.pathname, scrollTop: container.scrollTop })) } catch {}
}

function getActiveItem(container: HTMLElement) {
  const items = container.querySelectorAll<HTMLElement>('[data-active="true"]')
  let active: HTMLElement | null = null
  let activePathLength = -1
  for (const item of items) {
    const href = item.getAttribute("href") ?? item.querySelector<HTMLAnchorElement>("a[href]")?.getAttribute("href")
    if ((href?.length ?? 0) > activePathLength) { active = item; activePathLength = href?.length ?? 0 }
  }
  return active
}

function PageItem({ node, pathname }: { node: Extract<TreeNode, { type: "page" }>; pathname: string }) {
  return (
    <SidebarMenuItem>
      <SidebarMenuButton asChild isActive={node.url === pathname} className="relative h-[30px] w-fit overflow-visible border border-transparent text-[0.8rem] font-medium after:absolute after:inset-x-0 after:-inset-y-1 after:z-0 after:rounded-md data-[active=true]:border-accent data-[active=true]:bg-accent 3xl:fixed:w-full 3xl:fixed:max-w-48">
        <Link href={node.url}><span className="absolute inset-0 flex w-(--sidebar-menu-width) bg-transparent" />{node.name}</Link>
      </SidebarMenuButton>
    </SidebarMenuItem>
  )
}

function FolderItems({ node, pathname }: { node: Extract<TreeNode, { type: "folder" }>; pathname: string }) {
  return <>{node.children.map((child) => child.type === "page" ? <PageItem key={child.url} node={child as Extract<TreeNode, { type: "page" }>} pathname={pathname} /> : child.type === "folder" ? <FolderItems key={child.$id} node={child as Extract<TreeNode, { type: "folder" }>} pathname={pathname} /> : null)}</>
}

export function DocsSidebar({ tree, ...props }: React.ComponentProps<typeof Sidebar> & { tree: typeof source.pageTree }) {
  const pathname = usePathname()
  const contentRef = React.useRef<HTMLDivElement>(null)
  const activeProduct = pathname.split("/").filter(Boolean)[0]
  const activeFolder = tree.children.find((node) => node.type === "folder" && node.$id === activeProduct)

  React.useLayoutEffect(() => {
    const container = contentRef.current
    if (!container) return
    const scrollState = readScrollState()
    if (scrollState?.pathname === pathname) container.scrollTop = scrollState.scrollTop
    else getActiveItem(container)?.scrollIntoView({ block: "center" })
    saveScrollState(container)
  }, [pathname])

  React.useEffect(() => {
    const container = contentRef.current
    if (!container) return
    const onScroll = () => saveScrollState(container)
    container.addEventListener("scroll", onScroll, { passive: true })
    return () => container.removeEventListener("scroll", onScroll)
  }, [])

  return (
    <Sidebar className="sticky top-[calc(var(--header-height)+0.6rem)] z-30 hidden h-[calc(100svh-10rem)] overflow-hidden overscroll-none bg-transparent [--sidebar-menu-width:--spacing(56)] lg:flex" collapsible="none" {...props}>
      <div className="absolute top-12 right-2 bottom-0 hidden h-full w-px bg-[linear-gradient(to_bottom,transparent_0%,var(--border)_10%,var(--border)_90%,transparent_100%)] lg:flex" />
      <SidebarContent ref={contentRef} data-docs-sidebar-content="" className="w-(--sidebar-menu-width) scroll-fade scrollbar-none overflow-x-hidden pl-2.5">
        <SidebarGroup className="pt-12">
          <SidebarGroupLabel className="font-medium text-muted-foreground">Sections</SidebarGroupLabel>
          <SidebarGroupContent>
            <SidebarMenu>
              <SidebarMenuItem>
                <SidebarMenuButton asChild isActive={pathname === "/"} className="relative h-[30px] w-fit overflow-visible border border-transparent text-[0.8rem] font-medium after:absolute after:inset-x-0 after:-inset-y-1 after:z-0 after:rounded-md data-[active=true]:border-accent data-[active=true]:bg-accent 3xl:fixed:w-full 3xl:fixed:max-w-48">
                  <Link href="/"><span className="absolute inset-0 flex w-(--sidebar-menu-width) bg-transparent" />Overview</Link>
                </SidebarMenuButton>
              </SidebarMenuItem>
              {siteConfig.navItems.map(({ href, label }) => (
                <SidebarMenuItem key={href}>
                  <SidebarMenuButton asChild isActive={pathname === href || pathname.startsWith(`${href}/`)} className="relative h-[30px] w-fit overflow-visible border border-transparent text-[0.8rem] font-medium after:absolute after:inset-x-0 after:-inset-y-1 after:z-0 after:rounded-md data-[active=true]:border-accent data-[active=true]:bg-accent 3xl:fixed:w-full 3xl:fixed:max-w-48">
                    <Link href={href}><span className="absolute inset-0 flex w-(--sidebar-menu-width) bg-transparent" />{label}</Link>
                  </SidebarMenuButton>
                </SidebarMenuItem>
              ))}
            </SidebarMenu>
          </SidebarGroupContent>
        </SidebarGroup>
        {activeFolder?.type === "folder" ? (
          <SidebarGroup>
            <SidebarGroupLabel className="font-medium text-muted-foreground">{activeFolder.name}</SidebarGroupLabel>
            <SidebarGroupContent><SidebarMenu className="gap-0.5"><FolderItems node={activeFolder} pathname={pathname} /></SidebarMenu></SidebarGroupContent>
          </SidebarGroup>
        ) : null}
      </SidebarContent>
    </Sidebar>
  )
}
