import type {
  ExportedGeneric,
  ExportedItem,
  ExportedMember,
  ExportedParameter,
  ExportedSignature,
} from '@/lib/generated-content/package-export';
import type { CrossReference } from '@/lib/generated-content/schemas';

export type ReferenceTypeLink = CrossReference;

export interface MemberGroupData {
  id: string;
  kind: MemberKind;
  members: ExportedMember[];
  title: string;
}

export type MemberKind = 'associated-type' | 'field' | 'method' | 'variant';

export interface TypeDisplaySegment {
  reference?: ReferenceTypeLink;
  start: number;
  text: string;
}

export type TypeReferenceIndex = ReadonlyMap<string, ReferenceTypeLink>;

const TYPE_NAME_PATTERN =
  /[A-Za-z_$][A-Za-z0-9_$]*(?:\.[A-Za-z_$][A-Za-z0-9_$]*)*/g;

function parameterText(parameter: ExportedParameter): string {
  if (parameter.name === 'self') return 'self';
  return `${parameter.name}: ${parameter.ty.display}${parameter.optional ? ' = …' : ''}`;
}

function genericText(generic: ExportedGeneric): string {
  const bounds =
    generic.bounds.length > 0 ? ` extends ${generic.bounds.join(' & ')}` : '';
  return `${generic.name}${bounds}`;
}

export function genericParametersText(
  generics: readonly ExportedGeneric[] | undefined,
): string {
  return generics?.length ? `<${generics.map(genericText).join(', ')}>` : '';
}

export function signatureText(
  name: string,
  signature: ExportedSignature,
): string {
  const generics = genericParametersText(signature.generics);
  const parameters = signature.params.map(parameterText).join(', ');
  const throws =
    signature.throws && signature.throws.display !== 'never'
      ? ` throws ${signature.throws.display}`
      : '';
  return `${name}${generics}(${parameters}) -> ${signature.returns.display}${throws}`;
}

export function shouldUseMultilineSignature(
  name: string,
  signature: ExportedSignature,
): boolean {
  return (
    signature.params.length > 3 || signatureText(name, signature).length > 96
  );
}

export function splitMethods(members: ExportedMember[]) {
  const staticMethods: ExportedMember[] = [];
  const instanceMethods: ExportedMember[] = [];
  for (const member of members) {
    if (member.signature?.params[0]?.name === 'self') {
      instanceMethods.push(member);
    } else {
      staticMethods.push(member);
    }
  }
  return { instanceMethods, staticMethods };
}

export function declarationMemberGroups(
  declaration: ExportedItem,
): MemberGroupData[] {
  const declaredMethods = splitMethods(declaration.methods ?? []);
  const requiredMethods = splitMethods(declaration.required_methods ?? []);
  const defaultMethods = splitMethods(declaration.default_methods ?? []);
  return [
    {
      id: 'fields',
      kind: 'field' as const,
      members: declaration.fields ?? [],
      title: 'Fields',
    },
    {
      id: 'variants',
      kind: 'variant' as const,
      members: declaration.variants ?? [],
      title: 'Variants',
    },
    {
      id: 'associated-types',
      kind: 'associated-type' as const,
      members: declaration.assoc_types ?? [],
      title: 'Associated types',
    },
    {
      id: 'static-methods',
      kind: 'method' as const,
      members: declaredMethods.staticMethods,
      title: 'Static methods',
    },
    {
      id: 'required-static-methods',
      kind: 'method' as const,
      members: requiredMethods.staticMethods,
      title: 'Required static methods',
    },
    {
      id: 'default-static-methods',
      kind: 'method' as const,
      members: defaultMethods.staticMethods,
      title: 'Default static methods',
    },
    {
      id: 'instance-methods',
      kind: 'method' as const,
      members: declaredMethods.instanceMethods,
      title: 'Instance methods',
    },
    {
      id: 'required-instance-methods',
      kind: 'method' as const,
      members: requiredMethods.instanceMethods,
      title: 'Required instance methods',
    },
    {
      id: 'default-instance-methods',
      kind: 'method' as const,
      members: defaultMethods.instanceMethods,
      title: 'Default instance methods',
    },
  ].filter((group) => group.members.length > 0);
}

export function memberDeclarationText(
  member: ExportedMember,
  kind: MemberKind,
): string {
  if (member.signature) {
    return `function ${signatureText(member.name, member.signature)}`;
  }
  if (kind === 'associated-type') {
    const defaultType = member.default ? ` = ${member.default.display}` : '';
    return `type ${member.name}${defaultType}`;
  }
  return member.ty ? `${member.name}: ${member.ty.display}` : member.name;
}

export function createTypeReferenceIndex(
  references: readonly ReferenceTypeLink[],
  excludedQualifiedName?: string,
): TypeReferenceIndex {
  const index = new Map<string, ReferenceTypeLink>();
  for (const reference of references) {
    if (reference.qualified_name === excludedQualifiedName) continue;
    const existing = index.get(reference.qualified_name);
    if (!existing || (existing.anchor !== null && reference.anchor === null)) {
      index.set(reference.qualified_name, reference);
    }
  }
  return index;
}

export function typeDisplaySegments(
  display: string,
  references: TypeReferenceIndex,
): TypeDisplaySegment[] {
  const segments: TypeDisplaySegment[] = [];
  let plainTextStart = 0;

  for (const match of display.matchAll(TYPE_NAME_PATTERN)) {
    const reference = references.get(match[0]);
    if (!reference) continue;
    const start = match.index;
    if (plainTextStart < start) {
      segments.push({
        start: plainTextStart,
        text: display.slice(plainTextStart, start),
      });
    }
    segments.push({ reference, start, text: match[0] });
    plainTextStart = start + match[0].length;
  }

  if (plainTextStart < display.length) {
    segments.push({
      start: plainTextStart,
      text: display.slice(plainTextStart),
    });
  }

  return segments.length > 0 ? segments : [{ start: 0, text: display }];
}

export function referenceHref(
  routeVersion: string,
  reference: ReferenceTypeLink,
): string {
  const anchor = reference.anchor ? `#${reference.anchor}` : '';
  return `/baml/packages/${routeVersion}/${reference.route_path}${anchor}`;
}
