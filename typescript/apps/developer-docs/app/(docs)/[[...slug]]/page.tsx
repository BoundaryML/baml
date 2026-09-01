import Link from "next/link"
import { notFound } from "next/navigation"
import { IconArrowLeft, IconArrowRight } from "@tabler/icons-react"
import { findNeighbour } from "fumadocs-core/page-tree"

import { source } from "@/lib/source"
import { DocsTableOfContents } from "@/components/docs-toc"
import { Button } from "@/components/ui/button"
import { mdxComponents } from "@/mdx-components"

export const revalidate = false
export const dynamic = "force-static"
export const dynamicParams = false

export function generateStaticParams() { return source.generateParams() }

export async function generateMetadata({ params }: { params: Promise<{ slug?: string[] }> }) {
  const { slug } = await params
  const page = source.getPage(slug)
  if (!page) notFound()
  return { title: page.data.title, description: page.data.description, alternates: { canonical: page.url } }
}

export default async function Page({ params }: { params: Promise<{ slug?: string[] }> }) {
  const { slug } = await params
  const page = source.getPage(slug)
  if (!page) notFound()
  const MDX = page.data.body
  const neighbours = findNeighbour(source.pageTree, page.url)

  return (
    <div data-slot="docs" className="flex scroll-mt-24 items-stretch pb-8 text-[1.05rem] sm:text-[15px] xl:w-full">
      <div className="flex min-w-0 flex-1 flex-col">
        <div className="h-(--top-spacing) shrink-0" />
        <div className="mx-auto flex w-full max-w-160 min-w-0 flex-1 flex-col gap-6 px-4 py-6 text-foreground md:px-0 lg:py-8 dark:text-foreground">
          <div className="flex flex-col gap-2">
            <div className="flex flex-col gap-2">
              <div className="flex items-center justify-between md:items-start">
                <h1 className="scroll-m-24 text-3xl font-semibold tracking-tight sm:text-3xl">{page.data.title}</h1>
                <div className="docs-nav flex items-center gap-2">
                  <div className="ml-auto flex gap-2">
                    {neighbours.previous ? <Button variant="secondary" size="icon" className="extend-touch-target size-8 shadow-none md:size-7" asChild><Link href={neighbours.previous.url}><IconArrowLeft /><span className="sr-only">Previous</span></Link></Button> : null}
                    {neighbours.next ? <Button variant="secondary" size="icon" className="extend-touch-target size-8 shadow-none md:size-7" asChild><Link href={neighbours.next.url}><span className="sr-only">Next</span><IconArrowRight /></Link></Button> : null}
                  </div>
                </div>
              </div>
              {page.data.description ? <p className="text-[1.05rem] text-muted-foreground sm:text-base sm:text-balance md:max-w-[80%]">{page.data.description}</p> : null}
            </div>
          </div>
          <div className="typeset w-full flex-1 pb-16 sm:pb-0"><MDX components={mdxComponents} /></div>
          <div className="hidden h-16 w-full items-center gap-2 px-4 sm:flex sm:px-0">
            {neighbours.previous ? <Button variant="secondary" size="sm" asChild className="shadow-none"><Link href={neighbours.previous.url}><IconArrowLeft /> {neighbours.previous.name}</Link></Button> : null}
            {neighbours.next ? <Button variant="secondary" size="sm" className="ml-auto shadow-none" asChild><Link href={neighbours.next.url}>{neighbours.next.name} <IconArrowRight /></Link></Button> : null}
          </div>
        </div>
      </div>
      <div className="sticky top-[calc(var(--header-height)+1px)] z-30 ml-auto hidden h-[90svh] w-(--sidebar-width) flex-col gap-4 overflow-hidden overscroll-none pb-8 xl:flex">
        <div className="h-(--top-spacing) shrink-0" />
        {page.data.toc?.length ? <div className="flex scroll-fade scrollbar-none flex-col gap-8 overflow-y-auto px-8"><DocsTableOfContents toc={page.data.toc} /></div> : null}
      </div>
    </div>
  )
}
