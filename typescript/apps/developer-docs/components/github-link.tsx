import * as React from "react"
import Link from "next/link"

import { siteConfig } from "@/lib/config"
import { Icons } from "@/components/icons"
import { Button } from "@/components/ui/button"

export function GitHubLink() {
  return <Button asChild size="sm" variant="ghost" className="h-8 shadow-none"><Link href={siteConfig.github} target="_blank" rel="noreferrer"><Icons.gitHub /><React.Suspense fallback={<span className="h-4 w-[42px] animate-pulse rounded-md bg-accent" />}><StarsCount /></React.Suspense></Link></Button>
}

async function StarsCount() {
  let count: number | undefined
  try {
    const response = await fetch("https://api.github.com/repos/BoundaryML/baml", { next: { revalidate: 86400 } })
    if (response.ok) count = (await response.json()).stargazers_count
  } catch {}
  const formattedCount = count === undefined ? "—" : count >= 1000 ? `${Math.round(count / 1000)}k` : count.toLocaleString()
  return <span className="w-fit text-xs text-muted-foreground tabular-nums">{formattedCount}</span>
}
