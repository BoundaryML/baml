import Link from 'next/link';
import ReactMarkdown from 'react-markdown';
import { z } from 'zod';

import type { ReferencePageData } from '@/lib/generated-content/schemas';

const typeDisplaySchema = z
  .object({ display: z.string().min(1) })
  .passthrough();
const sourceSchema = z
  .object({
    end: z.number().int(),
    file: z.string().min(1),
    start: z.number().int(),
  })
  .passthrough();
const parameterSchema = z
  .object({
    name: z.string().min(1),
    optional: z.boolean().optional(),
    ty: typeDisplaySchema,
  })
  .passthrough();
const signatureSchema = z
  .object({
    params: z.array(parameterSchema),
    returns: typeDisplaySchema,
    throws: typeDisplaySchema.optional(),
  })
  .passthrough();
const genericSchema = z.object({ name: z.string().min(1) }).passthrough();
const memberSchema = z
  .object({
    docstring: z.string().optional(),
    id: z.string().min(1),
    name: z.string().min(1),
    signature: signatureSchema.optional(),
    ty: typeDisplaySchema.optional(),
  })
  .passthrough();
const declarationSchema = z
  .object({
    assoc_types: z.array(memberSchema).optional(),
    default_methods: z.array(memberSchema).optional(),
    detail: z.string().optional(),
    docstring: z.string().optional(),
    fields: z.array(memberSchema).optional(),
    generics: z.array(genericSchema).optional(),
    methods: z.array(memberSchema).optional(),
    name: z.string().min(1),
    required_methods: z.array(memberSchema).optional(),
    resolved: typeDisplaySchema.optional(),
    signature: signatureSchema.optional(),
    source: sourceSchema.optional(),
    variants: z.array(memberSchema).optional(),
  })
  .passthrough();
const implementationSchema = z
  .object({
    assoc_bindings: z
      .array(
        z
          .object({ name: z.string().min(1), ty: typeDisplaySchema })
          .passthrough(),
      )
      .optional(),
    docstring: z.string().optional(),
    for_ty: typeDisplaySchema.optional(),
    id: z.string().min(1),
    interface: z.string().optional(),
    methods: z.array(memberSchema).optional(),
    source: sourceSchema.optional(),
  })
  .passthrough();

type ExportedMember = z.output<typeof memberSchema>;
type ExportedSignature = z.output<typeof signatureSchema>;

export interface ReferenceChildLink {
  page_kind: ReferencePageData['page_kind'];
  qualified_name: string;
  route_path: string;
}

function Docstring({ value }: { value: string }) {
  return <ReactMarkdown>{value}</ReactMarkdown>;
}

function signatureText(name: string, signature: ExportedSignature): string {
  const parameters = signature.params
    .map(
      (parameter) =>
        `${parameter.name}${parameter.optional ? '?' : ''}: ${parameter.ty.display}`,
    )
    .join(', ');
  const throws =
    signature.throws && signature.throws.display !== 'never'
      ? ` throws ${signature.throws.display}`
      : '';
  return `${name}(${parameters}) -> ${signature.returns.display}${throws}`;
}

function SourceLocation({ source }: { source: z.output<typeof sourceSchema> }) {
  return (
    <p className="text-sm text-muted-foreground">
      Source: <code>{source.file}</code>, bytes {source.start}–{source.end}
    </p>
  );
}

function MemberGroup({
  anchors,
  members,
  title,
}: {
  anchors: Map<string, string>;
  members: ExportedMember[];
  title: string;
}) {
  if (members.length === 0) return null;
  return (
    <section>
      <h2>{title}</h2>
      {members.map((member) => (
        <article
          className="scroll-mt-24 border-t py-5 first:border-t-0"
          id={anchors.get(member.id)}
          key={member.id}
        >
          <h3>
            <code>{member.name}</code>
          </h3>
          {member.signature ? (
            <pre>
              <code>{signatureText(member.name, member.signature)}</code>
            </pre>
          ) : null}
          {member.ty ? (
            <pre>
              <code>
                {member.name}: {member.ty.display}
              </code>
            </pre>
          ) : null}
          {member.docstring ? <Docstring value={member.docstring} /> : null}
        </article>
      ))}
    </section>
  );
}

function ChildLinks({
  items,
  routeVersion,
}: {
  items: ReferenceChildLink[];
  routeVersion: string;
}) {
  return (
    <ul>
      {items.map((child) => (
        <li key={child.qualified_name}>
          <Link href={`/baml/packages/${routeVersion}/${child.route_path}`}>
            <code>{child.qualified_name}</code>
          </Link>{' '}
          <span className="text-muted-foreground">{child.page_kind}</span>
        </li>
      ))}
    </ul>
  );
}

export function referencePageTableOfContents(
  page: ReferencePageData,
  namespacedChildren: readonly ReferenceChildLink[] = [],
): { href: string; label: string }[] {
  if (page.page_kind === 'package' || page.page_kind === 'namespace') {
    return [{ href: '#contents', label: 'Contents' }];
  }
  return [
    { href: '#signature', label: 'Signature' },
    ...(page.member_anchors.length > 0
      ? [{ href: '#members', label: 'Members' }]
      : []),
    ...(page.implementations.length > 0
      ? [{ href: '#implementations', label: 'Implementations' }]
      : []),
    ...(page.cross_references.length > 0
      ? [{ href: '#related', label: 'Related definitions' }]
      : []),
    ...(namespacedChildren.length > 0
      ? [{ href: '#namespaced-definitions', label: 'Namespaced definitions' }]
      : []),
  ];
}

export function GeneratedReferenceContent({
  namespacedChildren = [],
  page,
  routeVersion,
}: {
  namespacedChildren?: ReferenceChildLink[];
  page: ReferencePageData;
  routeVersion: string;
}) {
  if (page.page_kind === 'package' || page.page_kind === 'namespace') {
    return (
      <section>
        <h2 id="contents">Contents</h2>
        {page.children.length > 0 ? (
          <ChildLinks items={page.children} routeVersion={routeVersion} />
        ) : (
          <p>This namespace has no directly routable children.</p>
        )}
      </section>
    );
  }

  const declaration = declarationSchema.parse(page.declaration);
  const implementations = implementationSchema
    .array()
    .parse(page.implementations);
  const anchors = new Map(
    page.member_anchors.map((anchor) => [anchor.exported_id, anchor.anchor]),
  );
  const genericSuffix = declaration.generics?.length
    ? `<${declaration.generics.map((generic) => generic.name).join(', ')}>`
    : '';
  const declarationSignature = declaration.signature
    ? signatureText(page.qualified_name, declaration.signature)
    : declaration.resolved
      ? `type ${page.qualified_name}${genericSuffix} = ${declaration.resolved.display}`
      : `${page.page_kind} ${page.qualified_name}${genericSuffix}`;
  const memberGroups: [string, ExportedMember[]][] = [
    ['Fields', declaration.fields ?? []],
    ['Variants', declaration.variants ?? []],
    ['Associated types', declaration.assoc_types ?? []],
    ['Required methods', declaration.required_methods ?? []],
    ['Default methods', declaration.default_methods ?? []],
    ['Methods', declaration.methods ?? []],
  ];

  return (
    <>
      <section>
        <h2 id="signature">Signature</h2>
        <pre>
          <code>{declarationSignature}</code>
        </pre>
        {declaration.docstring ? (
          <Docstring value={declaration.docstring} />
        ) : null}
        {declaration.source ? (
          <SourceLocation source={declaration.source} />
        ) : null}
      </section>
      {page.member_anchors.length > 0 ? (
        <div id="members">
          {memberGroups.map(([title, members]) => (
            <MemberGroup
              anchors={anchors}
              key={title}
              members={members}
              title={title}
            />
          ))}
        </div>
      ) : null}
      {implementations.length > 0 ? (
        <section>
          <h2 id="implementations">Implementations</h2>
          {implementations.map((implementation) => {
            const label = [
              implementation.interface,
              implementation.for_ty
                ? `for ${implementation.for_ty.display}`
                : null,
            ]
              .filter(Boolean)
              .join(' ');
            return (
              <article
                className="scroll-mt-24 border-t py-5 first:border-t-0"
                id={anchors.get(implementation.id)}
                key={implementation.id}
              >
                <h3>{label || 'Implementation'}</h3>
                {implementation.assoc_bindings?.length ? (
                  <p>
                    {implementation.assoc_bindings.map((binding) => (
                      <code key={binding.name}>
                        {binding.name} = {binding.ty.display}{' '}
                      </code>
                    ))}
                  </p>
                ) : null}
                {implementation.docstring ? (
                  <Docstring value={implementation.docstring} />
                ) : null}
                {implementation.methods?.map((method) => (
                  <article
                    className="scroll-mt-24 border-t py-4"
                    id={anchors.get(method.id)}
                    key={method.id}
                  >
                    <h4>
                      <code>{method.name}</code>
                    </h4>
                    {method.signature ? (
                      <pre>
                        <code>
                          {signatureText(method.name, method.signature)}
                        </code>
                      </pre>
                    ) : null}
                    {method.docstring ? (
                      <Docstring value={method.docstring} />
                    ) : null}
                  </article>
                ))}
                {implementation.source ? (
                  <SourceLocation source={implementation.source} />
                ) : null}
              </article>
            );
          })}
        </section>
      ) : null}
      {page.cross_references.length > 0 ? (
        <section>
          <h2 id="related">Related definitions</h2>
          <ul>
            {page.cross_references.map((reference) => (
              <li key={reference.exported_id}>
                <Link
                  href={`/baml/packages/${routeVersion}/${reference.route_path}${reference.anchor ? `#${reference.anchor}` : ''}`}
                >
                  <code>{reference.qualified_name}</code>
                </Link>
              </li>
            ))}
          </ul>
        </section>
      ) : null}
      {namespacedChildren.length > 0 ? (
        <section>
          <h2 id="namespaced-definitions">Namespaced definitions</h2>
          <p>
            These definitions use this declaration name as their namespace
            prefix.
          </p>
          <ChildLinks items={namespacedChildren} routeVersion={routeVersion} />
        </section>
      ) : null}
    </>
  );
}
