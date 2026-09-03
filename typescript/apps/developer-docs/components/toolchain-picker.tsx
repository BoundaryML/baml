"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname } from "next/navigation"
import { Check, ChevronsUpDown } from "lucide-react"

import { cn } from "@/lib/utils"
import { Button } from "@/components/ui/button"
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover"

export interface ToolchainOption {
  selector: string
  version: string
  track?: string
  channel: boolean
}

export function ToolchainPicker({ current, options, surface }: { current: string; options: ToolchainOption[]; surface: "packages" | "cli" }) {
  const pathname = usePathname()
  const [open, setOpen] = React.useState(false)
  const selected = options.find((option) => option.selector === current)

  function hrefFor(selector: string) {
    const segments = pathname.split("/").filter(Boolean)
    if (surface === "packages" && segments[0] === "baml" && segments[1] === "packages") {
      segments[2] = selector
      return `/${segments.join("/")}`
    }
    if (surface === "cli") {
      const known = new Set(options.map((option) => option.selector))
      if (segments[1] && known.has(segments[1])) segments[1] = selector
      else segments.splice(1, 0, selector)
      return `/${segments.join("/")}`
    }
    return pathname
  }

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button variant="outline" size="sm" className="h-8 min-w-44 justify-between rounded-lg bg-background px-2.5 font-mono text-xs shadow-none" aria-label="Choose toolchain">
          <span className="flex min-w-0 items-center gap-2">
            <span className="size-2 shrink-0 rounded-full bg-emerald-500" />
            <span className="truncate">{selected?.channel ? `${selected.selector} · ${selected.version}` : selected?.version ?? current}</span>
          </span>
          <ChevronsUpDown className="size-3.5 text-muted-foreground" />
        </Button>
      </PopoverTrigger>
      <PopoverContent align="start" className="w-64 p-1">
        <div className="px-2 py-1.5 text-[11px] font-medium uppercase tracking-wider text-muted-foreground">Toolchain</div>
        {options.map((option) => (
          <Link
            key={option.selector}
            href={hrefFor(option.selector)}
            onClick={() => setOpen(false)}
            className={cn("flex items-center gap-2 rounded-sm px-2 py-2 text-sm outline-none hover:bg-accent focus-visible:bg-accent", option.selector === current && "bg-accent/60")}
          >
            <span className="flex min-w-0 flex-1 flex-col">
              <span className="font-mono text-xs">{option.channel ? option.selector : option.version}</span>
              <span className="text-[11px] text-muted-foreground">{option.channel ? `${option.track ?? "release"} → ${option.version}` : "Exact release"}</span>
            </span>
            {option.selector === current ? <Check className="size-3.5" /> : null}
          </Link>
        ))}
      </PopoverContent>
    </Popover>
  )
}
