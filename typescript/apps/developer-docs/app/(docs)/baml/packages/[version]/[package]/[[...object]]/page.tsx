import Link from "next/link"
import { notFound, redirect } from "next/navigation"
import { IconArrowLeft } from "@tabler/icons-react"

import { ObjectReference, PackageLanding } from "@/components/package-reference"
import { ToolchainPicker } from "@/components/toolchain-picker"
import { Button } from "@/components/ui/button"
import {
  availableReferenceSelectors,
  directReferenceMembers,
  findReferenceItem,
  isPublicReferenceItem,
  loadPackageReference,
  referenceItemName,
  referenceItemPath,
  referencePackagePath,
  type ReferenceItem,
  type ResolvedPackageReference,
} from "@/lib/reference-data"

export const revalidate = false
export const dynamic = "force-static"

type PackagePageParams = { version: string; package: string; object?: string[] }

export async function generateStaticParams(): Promise<PackagePageParams[]> {
  const reference = await loadPackageReference("latest", "baml")
  if (!reference) return []
  const array = findReferenceItem(reference.dataset, ["Array"])
  if (!array) return []
  const objects = [array, ...directReferenceMembers(reference.dataset, array)]
  const selectors = ["latest", reference.resolvedVersion]
  return selectors.flatMap((version) => [
    { version, package: "baml" },
    ...objects.map((item) => ({ version, package: "baml", object: [...(item.namespace ?? []), item.name] })),
  ])
}

export async function generateMetadata({ params }: { params: Promise<PackagePageParams> }) {
  const resolved = await resolvePage(await params)
  if (!resolved) notFound()
  const title = resolved.item ? referenceItemName(resolved.item, resolved.reference.dataset.catalog.package) : resolved.reference.dataset.catalog.package
  return {
    title: `${title} package reference`,
    description: resolved.item?.summary ?? `Generated reference for the ${resolved.reference.dataset.catalog.package} package.`,
  }
}

async function resolvePage(params: PackagePageParams) {
  const reference = await loadPackageReference(params.version, params.package)
  if (!reference) {
    const selectorless = await loadPackageReference("latest", params.version)
    if (!selectorless) return null
    const shiftedSegments = [params.package, ...(params.object ?? [])]
    const shiftedItem = findReferenceItem(selectorless.dataset, shiftedSegments)
    if (!shiftedItem || !isPublicReferenceItem(shiftedItem)) return null
    redirect(referenceItemPath("latest", params.version, shiftedItem))
  }
  const segments = params.object ?? []
  if (segments.length === 0) return { reference, item: null }
  const item = findReferenceItem(reference.dataset, segments)
  if (!item || !isPublicReferenceItem(item)) return null
  return { reference, item }
}

function VersionContext({ reference, item, options }: { reference: ResolvedPackageReference; item: ReferenceItem | null; options: Awaited<ReturnType<typeof availableReferenceSelectors>> }) {
  const packageName = reference.dataset.catalog.package
  const suffix = item ? `/${[...(item.namespace ?? []), item.name].map(encodeURIComponent).join("/")}` : ""
  const exactHref = `${referencePackagePath(reference.resolvedVersion, packageName)}${suffix}`
  return (
    <div className="not-prose flex flex-wrap items-center gap-3 rounded-lg border bg-muted/25 px-3 py-2.5">
      <ToolchainPicker current={reference.requestedSelector} options={options} surface="packages" />
      <span className="text-xs text-muted-foreground">Package <code>{packageName}</code></span>
      {reference.channel ? <Link className="ml-auto text-xs font-medium underline underline-offset-4" href={exactHref}>Pin {reference.resolvedVersion}</Link> : <Link className="ml-auto text-xs font-medium underline underline-offset-4" href={`${referencePackagePath("latest", packageName)}${suffix}`}>Follow latest</Link>}
    </div>
  )
}

export default async function PackageReferencePage({ params }: { params: Promise<PackagePageParams> }) {
  const resolved = await resolvePage(await params)
  if (!resolved) notFound()
  const { reference, item } = resolved
  const packageName = reference.dataset.catalog.package
  const title = item ? referenceItemName(item, packageName) : packageName
  const description = item ? `${item.kind.replaceAll("_", " ")} in the ${packageName} package.` : `Compiler-generated reference for every public object in the ${packageName} package.`
  const parentSegments = item?.namespace ?? []
  const parent = parentSegments.length ? findReferenceItem(reference.dataset, parentSegments) : null
  const options = await availableReferenceSelectors()

  return (
    <div data-slot="docs" className="flex scroll-mt-24 items-stretch pb-8 text-[1.05rem] sm:text-[15px] xl:w-full">
      <div className="flex min-w-0 flex-1 flex-col">
        <div className="h-(--top-spacing) shrink-0" />
        <div className="mx-auto flex w-full max-w-160 min-w-0 flex-1 flex-col gap-6 px-4 py-6 text-foreground md:px-0 lg:py-8 dark:text-foreground">
          {parent ? <Button variant="ghost" size="sm" className="not-prose -mb-3 -ml-3 w-fit shadow-none" asChild><Link href={referenceItemPath(reference.requestedSelector, packageName, parent)}><IconArrowLeft />{referenceItemName(parent, packageName)}</Link></Button> : null}
          <div className="flex flex-col gap-2">
            <div className="font-mono text-xs text-muted-foreground">BAML / Packages / {reference.requestedSelector}</div>
            <h1 className="scroll-m-24 font-mono text-3xl font-semibold tracking-tight sm:text-3xl">{title}</h1>
            <p className="text-[1.05rem] text-muted-foreground sm:text-base sm:text-balance md:max-w-[90%]">{description}</p>
          </div>
          <VersionContext reference={reference} item={item} options={options} />
          <div className="typeset w-full min-w-0 flex-1 pb-16 sm:pb-0">{item ? <ObjectReference reference={reference} item={item} /> : <PackageLanding reference={reference} />}</div>
        </div>
      </div>
      <aside className="sticky top-[calc(var(--header-height)+1px)] z-30 ml-auto hidden h-[90svh] w-(--sidebar-width) flex-col gap-3 overflow-hidden px-8 pt-(--top-spacing) text-xs text-muted-foreground xl:flex">
        <p className="font-medium text-foreground">Generated reference</p>
        <p>Toolchain {reference.resolvedVersion}</p>
        <p>Track {reference.dataset.release.track}</p>
        {item ? <p className="font-mono break-all">{item.id}</p> : null}
        <p className="font-mono break-all" title={reference.dataset.release.artifact_digest}>source {reference.dataset.release.source_revision.slice(0, 12)}</p>
      </aside>
    </div>
  )
}
