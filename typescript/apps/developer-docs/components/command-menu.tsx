"use client"

import * as React from "react"
import { Search } from "lucide-react"
import * as DialogPrimitive from "@radix-ui/react-dialog"

import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"

export function CommandMenu(props: React.ComponentProps<typeof Button>) {
  const [open, setOpen] = React.useState(false)
  return (
    <DialogPrimitive.Root open={open} onOpenChange={setOpen}>
      <DialogPrimitive.Trigger asChild>
        <Button variant="outline" className={cn("relative h-8 w-full justify-start rounded-lg border-none bg-muted pl-3 text-foreground shadow-none transition-colors hover:bg-muted/50 md:w-48 lg:w-40 xl:w-64 dark:bg-card", props.className)} {...props}>
          <span className="hidden xl:inline-flex">Search documentation...</span>
          <span className="inline-flex xl:hidden">Search...</span>
        </Button>
      </DialogPrimitive.Trigger>
      <DialogPrimitive.Portal>
        <DialogPrimitive.Overlay className="fixed inset-0 z-50 bg-black/20 backdrop-blur-sm" />
        <DialogPrimitive.Content className="fixed top-[18%] left-1/2 z-50 w-[calc(100%-2rem)] max-w-xl -translate-x-1/2 rounded-xl border-none bg-background bg-clip-padding p-2 pb-12 shadow-2xl ring-4 ring-neutral-200/80 outline-none dark:bg-neutral-900 dark:ring-neutral-800">
          <DialogPrimitive.Title className="sr-only">Search documentation</DialogPrimitive.Title>
          <DialogPrimitive.Description className="sr-only">Search Boundary developer documentation.</DialogPrimitive.Description>
          <div className="flex h-10 items-center gap-2 rounded-md border border-input bg-input/50 px-3">
            <Search className="size-4 text-muted-foreground" />
            <input autoFocus placeholder="Search documentation..." className="h-full min-w-0 flex-1 bg-transparent text-sm outline-none placeholder:text-muted-foreground" />
          </div>
          <div className="flex min-h-64 items-center justify-center text-sm text-muted-foreground">Search indexing will move to Algolia; the docs route remains stable.</div>
          <div className="absolute inset-x-0 bottom-0 flex h-10 items-center rounded-b-xl border-t bg-neutral-50 px-4 text-xs font-medium text-muted-foreground dark:bg-neutral-800">Press Esc to close</div>
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  )
}
