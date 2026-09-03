"use client"

import * as React from "react"
import Link, { type LinkProps } from "next/link"
import { useRouter } from "next/navigation"

import { cn } from "@/lib/utils"
import { Button } from "@/components/ui/button"
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover"

export function MobileNav({ items, className }: { items: { href: string; label: string }[]; className?: string }) {
  const [open, setOpen] = React.useState(false)
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button variant="ghost" className={cn("extend-touch-target h-8 touch-manipulation items-center justify-start gap-2.5 p-0! hover:bg-transparent focus-visible:bg-transparent focus-visible:ring-0 active:bg-transparent dark:hover:bg-transparent", className)}>
          <div className="relative flex h-8 w-4 items-center justify-center">
            <div className="relative size-4">
              <span className={cn("absolute left-0 block h-0.5 w-4 bg-foreground transition-all duration-100", open ? "top-[0.4rem] -rotate-45" : "top-1")} />
              <span className={cn("absolute left-0 block h-0.5 w-4 bg-foreground transition-all duration-100", open ? "top-[0.4rem] rotate-45" : "top-2.5")} />
            </div>
            <span className="sr-only">Toggle Menu</span>
          </div>
          <span className="flex h-8 items-center text-lg leading-none font-medium">Menu</span>
        </Button>
      </PopoverTrigger>
      <PopoverContent className="no-scrollbar h-(--radix-popper-available-height) w-(--radix-popper-available-width) overflow-y-auto rounded-none border-none bg-background/90 p-0 shadow-none backdrop-blur duration-100 data-open:animate-none!" align="start" side="bottom" alignOffset={-16} sideOffset={14}>
        <div className="flex flex-col gap-12 overflow-auto px-6 py-6">
          <div className="flex flex-col gap-4">
            <div className="text-sm font-medium text-muted-foreground">Menu</div>
            <div className="flex flex-col gap-3">{items.map((item) => <MobileLink key={item.href} href={item.href} onOpenChange={setOpen}>{item.label}</MobileLink>)}</div>
          </div>
        </div>
      </PopoverContent>
    </Popover>
  )
}

function MobileLink({ href, onOpenChange, children, className, ...props }: LinkProps & { onOpenChange?: (open: boolean) => void; children: React.ReactNode; className?: string }) {
  const router = useRouter()
  return <Link href={href} onClick={() => { router.push(href.toString()); onOpenChange?.(false) }} className={cn("flex items-center gap-2 text-2xl font-medium", className)} {...props}>{children}</Link>
}
