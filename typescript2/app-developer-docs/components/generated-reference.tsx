import Link from 'next/link';
import ReactMarkdown from 'react-markdown';

import { GeneratedMemberActions } from '@/components/generated-member-actions';
import {
  type ExportedGeneric,
  type ExportedMember,
  type ExportedParameter,
  type ExportedSignature,
  type ExportedSource,
  exportedImplementationSchema,
  exportedItemSchema,
} from '@/lib/generated-content/package-export';
import {
  createTypeReferenceIndex,
  declarationMemberGroups,
  type MemberKind,
  memberDeclarationText,
  type ReferenceTypeLink,
  referenceHref,
  shouldUseMultilineSignature,
  splitMethods,
  type TypeReferenceIndex,
  typeDisplaySegments,
} from '@/lib/generated-content/reference-rendering';
import type { ReferencePageData } from '@/lib/generated-content/schemas';

export interface ReferenceChildLink {
  page_kind: ReferencePageData['page_kind'];
  qualified_name: string;
  route_path: string;
}

function Docstring({ value }: { value: string }) {
  return <ReactMarkdown>{value}</ReactMarkdown>;
}

interface TypeLinkContext {
  references: TypeReferenceIndex;
  routeVersion: string;
}

function TypeDisplay({
  links,
  value,
}: {
  links: TypeLinkContext;
  value: string;
}) {
  return typeDisplaySegments(value, links.references).map((segment) => {
    if (!segment.reference) return segment.text;
    return (
      <Link
        className="font-medium text-foreground underline decoration-border underline-offset-2 transition-colors hover:decoration-foreground"
        href={referenceHref(links.routeVersion, segment.reference)}
        key={`${segment.reference.exported_id}-${segment.start}`}
      >
        {segment.text}
      </Link>
    );
  });
}

function GenericParameters({
  generics,
  links,
}: {
  generics: readonly ExportedGeneric[] | undefined;
  links: TypeLinkContext;
}) {
  if (!generics?.length) return null;
  return (
    <>
      {'<'}
      {generics.map((generic, genericIndex) => (
        <span key={generic.name}>
          {genericIndex > 0 ? ', ' : null}
          {generic.name}
          {generic.bounds.length > 0 ? ' extends ' : null}
          {generic.bounds.map((bound, boundIndex) => (
            <span key={bound}>
              {boundIndex > 0 ? ' & ' : null}
              <TypeDisplay links={links} value={bound} />
            </span>
          ))}
        </span>
      ))}
      {'>'}
    </>
  );
}

function ParameterDisplay({
  links,
  parameter,
}: {
  links: TypeLinkContext;
  parameter: ExportedParameter;
}) {
  if (parameter.name === 'self') return 'self';
  return (
    <>
      {parameter.name}:{' '}
      <TypeDisplay links={links} value={parameter.ty.display} />
      {parameter.optional ? ' = …' : null}
    </>
  );
}

function FunctionSignature({
  headingLevel,
  links,
  name,
  signature,
}: {
  headingLevel?: 'h3' | 'h4';
  links: TypeLinkContext;
  name: string;
  signature: ExportedSignature;
}) {
  const Name = headingLevel ?? 'span';
  const thrownType =
    signature.throws?.display === 'never' ? null : signature.throws?.display;
  const multiline = shouldUseMultilineSignature(name, signature);
  const nameElement = (
    <Name className="inline font-mono text-[0.82rem] leading-5 font-semibold text-foreground">
      {name}
    </Name>
  );

  if (!multiline) {
    return (
      <div className="min-w-0 font-mono text-[0.82rem] leading-5 break-words whitespace-pre-wrap">
        <span className="font-medium text-[var(--docs-purple)]">function </span>
        {nameElement}
        <span className="text-muted-foreground">
          <GenericParameters generics={signature.generics} links={links} />(
        </span>
        {signature.params.map((parameter, index) => (
          <span className="text-muted-foreground" key={parameter.name}>
            {index > 0 ? ', ' : null}
            <ParameterDisplay links={links} parameter={parameter} />
          </span>
        ))}
        <span className="text-muted-foreground">) -&gt; </span>
        <span className="text-muted-foreground">
          <TypeDisplay links={links} value={signature.returns.display} />
        </span>
        {thrownType ? (
          <>
            {' '}
            <span className="font-medium text-[var(--docs-purple)]">
              throws
            </span>{' '}
            <span className="text-muted-foreground">
              <TypeDisplay links={links} value={thrownType} />
            </span>
          </>
        ) : null}
      </div>
    );
  }

  return (
    <div className="min-w-0 font-mono text-[0.82rem] leading-5 break-words">
      <div>
        <span className="font-medium text-[var(--docs-purple)]">function </span>
        {nameElement}
        <span className="text-muted-foreground">
          <GenericParameters generics={signature.generics} links={links} />(
        </span>
      </div>
      <div className="pl-4 text-muted-foreground">
        {signature.params.map((parameter, index) => (
          <div key={parameter.name}>
            <ParameterDisplay links={links} parameter={parameter} />
            {index < signature.params.length - 1 ? ',' : null}
          </div>
        ))}
      </div>
      <div className="text-muted-foreground">
        ) -&gt; <TypeDisplay links={links} value={signature.returns.display} />
        {thrownType ? (
          <>
            {' '}
            <span className="font-medium text-[var(--docs-purple)]">
              throws
            </span>{' '}
            <TypeDisplay links={links} value={thrownType} />
          </>
        ) : null}
      </div>
    </div>
  );
}

function SourceLocation({ source }: { source: ExportedSource }) {
  return (
    <p className="flex flex-wrap items-baseline gap-x-1 text-sm text-muted-foreground">
      <span>Source:</span>
      <code>{source.file}</code>
      <span>
        bytes {source.start}–{source.end}
      </span>
    </p>
  );
}

function CompactDocstring({ value }: { value: string }) {
  return (
    <div className="mt-2 text-sm leading-6 text-muted-foreground [&_a]:underline [&_li]:mt-1 [&_ol]:mt-2 [&_ol]:list-decimal [&_ol]:pl-5 [&_p]:m-0 [&_ul]:mt-2 [&_ul]:list-disc [&_ul]:pl-5">
      <ReactMarkdown>{value}</ReactMarkdown>
    </div>
  );
}

function MemberRow({
  anchor,
  headingLevel = 'h3',
  kind,
  links,
  member,
}: {
  anchor?: string;
  headingLevel?: 'h3' | 'h4';
  kind: MemberKind;
  links: TypeLinkContext;
  member: ExportedMember;
}) {
  const Heading = headingLevel;

  const declaration = memberDeclarationText(member, kind);
  if (member.signature) {
    return (
      <article
        className="group/member relative scroll-mt-24 min-w-0 px-4 py-3 pr-20 transition-colors hover:bg-muted/25"
        id={anchor}
      >
        <FunctionSignature
          headingLevel={headingLevel}
          links={links}
          name={member.name}
          signature={member.signature}
        />
        {anchor ? (
          <GeneratedMemberActions
            anchor={anchor}
            declaration={declaration}
            label={member.name}
          />
        ) : null}
        {member.docstring ? (
          <CompactDocstring value={member.docstring} />
        ) : null}
      </article>
    );
  }

  const displayedType = member.ty ?? member.default;
  return (
    <article
      className="group/member relative scroll-mt-24 grid min-w-0 gap-1.5 px-4 py-3 pr-20 transition-colors hover:bg-muted/25 sm:grid-cols-[minmax(12rem,0.65fr)_minmax(0,1fr)] sm:gap-4"
      id={anchor}
    >
      <Heading className="min-w-0 font-mono text-sm leading-5 font-medium text-foreground">
        {kind === 'associated-type' ? (
          <span className="text-[var(--docs-purple)]">type </span>
        ) : null}
        {member.name}
      </Heading>
      <div className="min-w-0">
        {displayedType ? (
          <code className="font-mono text-xs leading-5 text-muted-foreground">
            {kind === 'associated-type' ? '= ' : null}
            <TypeDisplay links={links} value={displayedType.display} />
          </code>
        ) : null}
        {member.docstring ? (
          <CompactDocstring value={member.docstring} />
        ) : null}
      </div>
      {anchor ? (
        <GeneratedMemberActions
          anchor={anchor}
          declaration={declaration}
          label={member.name}
        />
      ) : null}
    </article>
  );
}

function MemberRows({
  anchors,
  headingLevel,
  kind,
  links,
  members,
}: {
  anchors: Map<string, string>;
  headingLevel?: 'h3' | 'h4';
  kind: MemberKind;
  links: TypeLinkContext;
  members: ExportedMember[];
}) {
  return members.map((member) => (
    <MemberRow
      anchor={anchors.get(member.id)}
      headingLevel={headingLevel}
      key={member.id}
      kind={kind}
      links={links}
      member={member}
    />
  ));
}

function MemberGroup({
  anchors,
  id,
  kind,
  links,
  members,
  title,
}: {
  anchors: Map<string, string>;
  id: string;
  kind: MemberKind;
  links: TypeLinkContext;
  members: ExportedMember[];
  title: string;
}) {
  return (
    <section className="mt-8" data-not-typeset="">
      <h2
        className="scroll-mt-24 text-xl leading-7 font-semibold tracking-tight"
        id={id}
      >
        {title}
      </h2>
      <div className="mt-3 divide-y overflow-hidden rounded-lg border bg-background">
        <MemberRows
          anchors={anchors}
          kind={kind}
          links={links}
          members={members}
        />
      </div>
    </section>
  );
}

function ImplementationMethodRows({
  anchors,
  links,
  members,
  title,
}: {
  anchors: Map<string, string>;
  links: TypeLinkContext;
  members: ExportedMember[];
  title: string;
}) {
  if (members.length === 0) return null;
  return (
    <div className="border-t">
      <p className="bg-muted/15 px-4 py-2 text-[0.7rem] font-medium tracking-wider text-muted-foreground uppercase">
        {title}
      </p>
      <div className="divide-y border-t">
        <MemberRows
          anchors={anchors}
          headingLevel="h4"
          kind="method"
          links={links}
          members={members}
        />
      </div>
    </div>
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
  const declaration = exportedItemSchema.parse(page.declaration);
  const memberGroups = declarationMemberGroups(declaration);
  return [
    { href: '#signature', label: 'Signature' },
    ...memberGroups.map((group) => ({
      href: `#${group.id}`,
      label: group.title,
    })),
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
  typeReferences = [],
}: {
  namespacedChildren?: ReferenceChildLink[];
  page: ReferencePageData;
  routeVersion: string;
  typeReferences?: readonly ReferenceTypeLink[];
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

  const declaration = exportedItemSchema.parse(page.declaration);
  const implementations = exportedImplementationSchema
    .array()
    .parse(page.implementations);
  const anchors = new Map(
    page.member_anchors.map((anchor) => [anchor.exported_id, anchor.anchor]),
  );
  const memberGroups = declarationMemberGroups(declaration);
  const links: TypeLinkContext = {
    references: createTypeReferenceIndex(
      [...page.cross_references, ...typeReferences],
      page.qualified_name,
    ),
    routeVersion,
  };

  return (
    <>
      <section>
        <h2 id="signature">Signature</h2>
        <div
          className="overflow-x-auto rounded-lg bg-muted/40 px-4 py-3"
          data-not-typeset=""
        >
          {declaration.signature ? (
            <FunctionSignature
              links={links}
              name={page.qualified_name}
              signature={declaration.signature}
            />
          ) : (
            <code className="font-mono text-[0.82rem] leading-5 text-muted-foreground">
              {declaration.resolved ? (
                <>
                  type {page.qualified_name}
                  <GenericParameters
                    generics={declaration.generics}
                    links={links}
                  />{' '}
                  ={' '}
                  <TypeDisplay
                    links={links}
                    value={declaration.resolved.display}
                  />
                </>
              ) : (
                <>
                  {page.page_kind} {page.qualified_name}
                  <GenericParameters
                    generics={declaration.generics}
                    links={links}
                  />
                </>
              )}
            </code>
          )}
        </div>
        {declaration.docstring ? (
          <Docstring value={declaration.docstring} />
        ) : null}
        {declaration.source ? (
          <SourceLocation source={declaration.source} />
        ) : null}
      </section>
      {memberGroups.length > 0 ? (
        <div id="members">
          {memberGroups.map((group) => (
            <MemberGroup
              anchors={anchors}
              id={group.id}
              key={group.id}
              kind={group.kind}
              links={links}
              members={group.members}
              title={group.title}
            />
          ))}
        </div>
      ) : null}
      {implementations.length > 0 ? (
        <section className="mt-8" data-not-typeset="">
          <h2
            className="text-xl leading-7 font-semibold tracking-tight"
            id="implementations"
          >
            Implementations
          </h2>
          <div className="mt-3 space-y-3">
            {implementations.map((implementation) => {
              const implementationMethods = splitMethods(
                implementation.methods ?? [],
              );
              return (
                <article
                  className="scroll-mt-24 overflow-hidden rounded-lg border bg-background"
                  id={anchors.get(implementation.id)}
                  key={implementation.id}
                >
                  <header className="bg-muted/35 px-4 py-3">
                    <h3 className="text-sm leading-5 font-semibold break-words">
                      {implementation.interface ? (
                        <TypeDisplay
                          links={links}
                          value={implementation.interface}
                        />
                      ) : (
                        'Implementation'
                      )}
                      {implementation.for_ty ? (
                        <>
                          {' '}
                          for{' '}
                          <TypeDisplay
                            links={links}
                            value={implementation.for_ty.display}
                          />
                        </>
                      ) : null}
                    </h3>
                    {implementation.assoc_bindings?.length ? (
                      <div className="mt-2 flex flex-wrap gap-2">
                        {implementation.assoc_bindings.map((binding) => (
                          <code
                            className="rounded-md bg-muted px-2 py-1 font-mono text-xs"
                            key={binding.name}
                          >
                            {binding.name} ={' '}
                            <TypeDisplay
                              links={links}
                              value={binding.ty.display}
                            />
                          </code>
                        ))}
                      </div>
                    ) : null}
                    {implementation.docstring ? (
                      <CompactDocstring value={implementation.docstring} />
                    ) : null}
                  </header>
                  <ImplementationMethodRows
                    anchors={anchors}
                    links={links}
                    members={implementationMethods.staticMethods}
                    title="Static methods"
                  />
                  <ImplementationMethodRows
                    anchors={anchors}
                    links={links}
                    members={implementationMethods.instanceMethods}
                    title="Instance methods"
                  />
                  {implementation.source ? (
                    <div className="border-t px-4 py-2.5">
                      <SourceLocation source={implementation.source} />
                    </div>
                  ) : null}
                </article>
              );
            })}
          </div>
        </section>
      ) : null}
      {page.cross_references.length > 0 ? (
        <section>
          <h2 id="related">Related definitions</h2>
          <ul>
            {page.cross_references.map((reference) => (
              <li key={reference.exported_id}>
                <Link href={referenceHref(routeVersion, reference)}>
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
