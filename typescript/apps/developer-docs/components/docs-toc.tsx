"use client"

import { useEffect, useMemo, useState } from "react"

export function DocsTableOfContents({
  toc,
}: {
  toc: { title?: React.ReactNode; url: string; depth: number }[]
}) {
  const ids = useMemo(() => toc.map((item) => item.url.replace(/^#/, "")), [toc])
  const [activeId, setActiveId] = useState<string | null>(null)

  useEffect(() => {
    const observer = new IntersectionObserver(
      (entries) => {
        const visible = entries.find((entry) => entry.isIntersecting)
        if (visible) setActiveId(visible.target.id)
      },
      { rootMargin: "0% 0% -80% 0%" }
    )

    ids.forEach((id) => {
      const element = document.getElementById(id)
      if (element) observer.observe(element)
    })
    return () => observer.disconnect()
  }, [ids])

  if (!toc.length) return null

  return (
    <nav className="flex flex-col gap-2 p-4 pt-0 text-sm" aria-label="On this page">
      <p className="h-6 bg-background text-xs font-medium text-muted-foreground">On This Page</p>
      {toc.map((item) => (
        <a
          key={item.url}
          href={item.url}
          data-active={item.url === `#${activeId}`}
          data-depth={item.depth}
          className="text-[0.8rem] text-muted-foreground no-underline transition-colors hover:text-foreground data-[active=true]:font-medium data-[active=true]:text-foreground data-[depth=3]:pl-4 data-[depth=4]:pl-6"
        >
          {item.title}
        </a>
      ))}
    </nav>
  )
}
