import * as React from "react"
import Link from "next/link"

import { highlightBaml } from "@/lib/baml-highlight"
import {
  describeImplementations,
  directReferenceMembers,
  findDescribeItem,
  findDescribeRecord,
  formatReferenceSignature,
  isPublicReferenceItem,
  referenceItemName,
  referenceItemPath,
  type DescribeGeneric,
  type DescribeItem,
  type DescribeRecord,
  type DescribeSignature,
  type DescribeType,
  type ReferenceDataset,
  type ReferenceItem,
  type ResolvedPackageReference,
} from "@/lib/reference-data"

function generics(value?: DescribeGeneric[]) {
  return value?.length ? `<${value.map((generic) => generic.name).join(", ")}>` : ""
}

function signatureDeclaration(name: string, signature: DescribeSignature) {
  const params = signature.params.map((param) => `${param.name}${param.optional ? "?" : ""}: ${param.ty.display}`).join(", ")
  const throws = signature.throws.display === "never" ? "" : ` throws ${signature.throws.display}`
  return `function ${name}${generics(signature.generics)}(${params}) -> ${signature.returns.display}${throws}`
}

export function referenceDeclaration(item: ReferenceItem, describe?: DescribeItem | DescribeRecord) {
  if (describe?.signature) return signatureDeclaration(describe.name, describe.signature)
  if (describe && "kind" in describe) {
    const describedItem = describe as DescribeItem
    if (describedItem.kind === "type_alias") return `type ${describedItem.name}${generics(describedItem.generics)} = ${describedItem.resolved?.display ?? "unknown"}`
    return `${describedItem.detail ?? describedItem.kind} ${describedItem.name}${generics(describedItem.generics)}`
  }
  if (item.kind === "field" && describe?.ty) return `${describe.name}: ${describe.ty.display}`
  if (item.kind === "associated_type") return `type ${describe?.name ?? item.name}${describe?.default ? ` = ${describe.default.display}` : ""}`
  if (item.kind === "variant") return describe?.name ?? item.name
  return formatReferenceSignature(item) ?? `${item.kind} ${item.name}`
}

async function HighlightedDeclaration({ code, compact = false }: { code: string; compact?: boolean }) {
  const html = await highlightBaml(code)
  return <div className={compact ? "not-prose baml-reference-highlight baml-reference-highlight-compact" : "not-prose baml-reference-highlight"} dangerouslySetInnerHTML={{ __html: html }} />
}

function InlineDoc({ children }: { children: string }) {
  const parts = children.split(/(`[^`]+`|\*\*[^*]+\*\*)/g)
  return <>{parts.map((part, index) => part.startsWith("`") && part.endsWith("`") ? <code key={index}>{part.slice(1, -1)}</code> : part.startsWith("**") && part.endsWith("**") ? <strong key={index}>{part.slice(2, -2)}</strong> : part)}</>
}

function DocText({ value }: { value: string }) {
  const lines = value.split("\n")
  const nodes: React.ReactNode[] = []
  let paragraph: string[] = []
  let list: string[] = []
  const flushParagraph = () => {
    if (paragraph.length) nodes.push(<p key={`p-${nodes.length}`}><InlineDoc>{paragraph.join(" ")}</InlineDoc></p>)
    paragraph = []
  }
  const flushList = () => {
    if (list.length) nodes.push(<ul key={`ul-${nodes.length}`}>{list.map((line, index) => <li key={index}><InlineDoc>{line}</InlineDoc></li>)}</ul>)
    list = []
  }
  for (const line of lines) {
    if (!line.trim()) { flushParagraph(); flushList(); continue }
    const heading = /^(#{1,4})\s+(.+)$/.exec(line)
    if (heading) {
      flushParagraph(); flushList()
      nodes.push(<h3 key={`h-${nodes.length}`} className="mt-8 scroll-m-24 text-base font-semibold"><InlineDoc>{heading[2] ?? ""}</InlineDoc></h3>)
    } else if (line.startsWith("- ")) {
      flushParagraph(); list.push(line.slice(2))
    } else {
      flushList(); paragraph.push(line.trim())
    }
  }
  flushParagraph(); flushList()
  return <>{nodes}</>
}

async function ReferenceDocstring({ value }: { value?: string }) {
  if (!value) return null
  const pieces = value.split(/```(?:baml)?\n([\s\S]*?)```/g)
  return <div className="reference-docstring">{await Promise.all(pieces.map(async (piece, index) => index % 2 ? <HighlightedDeclaration key={index} code={piece.trimEnd()} /> : <DocText key={index} value={piece} />))}</div>
}

function TypeValue({ value, dataset, selector }: { value: DescribeType; dataset: ReferenceDataset; selector: string }) {
  const target = value.head ? dataset.catalog.items.find((item) => item.id === value.head && isPublicReferenceItem(item)) : undefined
  return target ? <Link className="font-mono text-xs underline underline-offset-4" href={referenceItemPath(selector, dataset.catalog.package, target)}>{value.display}</Link> : <code className="text-xs">{value.display}</code>
}

function SignatureFacts({ signature, dataset, selector }: { signature?: DescribeSignature; dataset: ReferenceDataset; selector: string }) {
  if (!signature) return null
  return (
    <div className="not-prose mt-8 overflow-hidden rounded-lg border">
      {signature.params.length ? <div className="grid grid-cols-[minmax(7rem,0.5fr)_minmax(0,1fr)] border-b bg-muted/20 px-4 py-2 text-[11px] font-medium uppercase tracking-wider text-muted-foreground"><span>Parameter</span><span>Type</span></div> : null}
      {signature.params.map((param) => <div key={param.name} className="grid grid-cols-[minmax(7rem,0.5fr)_minmax(0,1fr)] border-b px-4 py-3"><code className="text-xs font-semibold">{param.name}{param.optional ? "?" : ""}</code><TypeValue value={param.ty} dataset={dataset} selector={selector} /></div>)}
      <div className="grid grid-cols-[minmax(7rem,0.5fr)_minmax(0,1fr)] border-b px-4 py-3"><span className="text-xs font-medium text-muted-foreground">Returns</span><TypeValue value={signature.returns} dataset={dataset} selector={selector} /></div>
      <div className="grid grid-cols-[minmax(7rem,0.5fr)_minmax(0,1fr)] px-4 py-3"><span className="text-xs font-medium text-muted-foreground">Throws</span><TypeValue value={signature.throws} dataset={dataset} selector={selector} /></div>
    </div>
  )
}

function Generics({ values }: { values?: DescribeGeneric[] }) {
  if (!values?.length) return null
  return <section><h2>Type parameters</h2><div className="not-prose divide-y rounded-lg border">{values.map((generic) => <div key={generic.name} className="grid grid-cols-[7rem_1fr] px-4 py-3 text-sm"><code className="font-semibold">{generic.name}</code><span className="text-muted-foreground">{generic.bounds.length ? generic.bounds.join(" + ") : "Any type"}</span></div>)}</div></section>
}

function SourceFact({ source }: { source?: { file: string; start: number; end: number } }) {
  if (!source) return null
  return <p className="not-prose mt-8 text-xs text-muted-foreground">Defined in <code>{source.file}</code> · bytes {source.start}–{source.end}</p>
}

export async function PackageLanding({ reference }: { reference: ResolvedPackageReference }) {
  const packageName = reference.dataset.catalog.package
  const describedIds = new Set(reference.dataset.describe.items.map((item) => item.id))
  const objects = reference.dataset.catalog.items.filter((item) => describedIds.has(item.id) && isPublicReferenceItem(item))
  const groups = objects.reduce((result, item) => {
    const key = item.namespace?.length ? `${packageName}.${item.namespace.join(".")}` : packageName
    const entries = result.get(key) ?? []
    entries.push(item)
    result.set(key, entries)
    return result
  }, new Map<string, ReferenceItem[]>())
  return (
    <>
      <p>All <strong>{objects.length} public objects</strong> and <strong>{reference.dataset.catalog.catalog_item_count} routable symbols</strong> exported by <code>{reference.dataset.producer.command}</code>. Select an object to see its declaration, complete documentation, members, relationships, and source metadata.</p>
      {[...groups.entries()].map(([namespace, items]) => <section key={namespace}><h2 className="font-mono">{namespace} <span className="text-sm font-normal text-muted-foreground">({items.length})</span></h2><div className="not-prose grid gap-px overflow-hidden rounded-lg border bg-border sm:grid-cols-2">{items.map((item) => <Link key={item.id} href={referenceItemPath(reference.requestedSelector, packageName, item)} className="bg-background p-4 hover:bg-muted/50"><div className="flex items-baseline justify-between gap-3"><span className="font-mono text-sm font-semibold">{item.name}</span><span className="text-[10px] uppercase tracking-wide text-muted-foreground">{item.kind.replaceAll("_", " ")}</span></div>{item.summary ? <p className="mt-1 line-clamp-2 text-xs leading-5 text-muted-foreground">{item.summary}</p> : null}<p className="mt-2 text-[11px] text-muted-foreground">{item.member_count} members</p></Link>)}</div></section>)}
    </>
  )
}

export async function ObjectReference({ reference, item }: { reference: ResolvedPackageReference; item: ReferenceItem }) {
  const dataset = reference.dataset
  const packageName = dataset.catalog.package
  const describe = findDescribeRecord(dataset, item.id)
  const topLevel = describe && "kind" in describe ? describe as DescribeItem : undefined
  const members = directReferenceMembers(dataset, item)
  const highlightedMembers = await Promise.all(members.map(async (member) => ({ member, describe: findDescribeRecord(dataset, member.id), code: referenceDeclaration(member, findDescribeRecord(dataset, member.id)) })))
  const implementations = topLevel ? describeImplementations(dataset, topLevel) : []
  const implementationsById = new Map(implementations.map((implementation) => [implementation.id, implementation]))
  const implementationIds = topLevel?.impls ?? []

  return (
    <>
      <HighlightedDeclaration code={referenceDeclaration(item, describe)} />
      <ReferenceDocstring value={describe?.docstring ?? item.summary} />
      <Generics values={topLevel?.generics ?? describe?.signature?.generics} />
      <SignatureFacts signature={describe?.signature} dataset={dataset} selector={reference.requestedSelector} />
      {members.length ? <section><h2>Members <span className="text-sm font-normal text-muted-foreground">({members.length})</span></h2><div className="not-prose divide-y overflow-hidden rounded-lg border">{highlightedMembers.map(({ member, describe: memberDescribe, code }) => <article className="grid gap-2 p-4" key={member.id}><div className="flex items-start justify-between gap-4"><Link className="font-mono text-sm font-semibold underline underline-offset-4" href={referenceItemPath(reference.requestedSelector, packageName, member)}>{member.name}</Link><span className="text-[10px] font-medium uppercase tracking-wide text-muted-foreground">{member.kind.replaceAll("_", " ")}</span></div><HighlightedDeclaration compact code={code} />{memberDescribe?.docstring ? <p className="m-0 line-clamp-2 text-sm leading-6 text-muted-foreground">{memberDescribe.docstring.split("\n")[0]}</p> : null}</article>)}</div></section> : null}
      {implementationIds.length ? <section><h2>Implementations <span className="text-sm font-normal text-muted-foreground">({implementationIds.length})</span></h2><div className="not-prose divide-y rounded-lg border">{implementationIds.map((id) => { const implementation = implementationsById.get(id); return implementation ? <div key={id} className="grid gap-1 px-4 py-3"><code className="text-xs font-semibold">{implementation.interface ?? implementation.id}</code><span className="text-xs text-muted-foreground">for {implementation.for_ty.display}</span>{implementation.assoc_bindings?.map((binding) => <span key={binding.name} className="text-xs text-muted-foreground">{binding.name} = {binding.ty.display}</span>)}</div> : <div key={id} className="grid gap-1 px-4 py-3"><code className="text-xs font-semibold">{id}</code><span className="text-xs text-muted-foreground">Defined by another package export</span></div> })}</div></section> : null}
      {topLevel?.implementors?.length ? <section><h2>Implementors</h2><div className="not-prose flex flex-wrap gap-2">{topLevel.implementors.map((id) => { const target = dataset.catalog.items.find((candidate) => candidate.id === id); return target ? <Link key={id} className="rounded-md border px-2 py-1 font-mono text-xs hover:bg-muted" href={referenceItemPath(reference.requestedSelector, packageName, target)}>{referenceItemName(target, packageName)}</Link> : <code key={id} className="rounded-md border px-2 py-1 text-xs">{id}</code> })}</div></section> : null}
      <SourceFact source={describe?.source} />
    </>
  )
}
