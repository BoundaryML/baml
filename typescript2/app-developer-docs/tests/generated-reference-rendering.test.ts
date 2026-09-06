import assert from 'node:assert/strict';
import test from 'node:test';
import { referencePageTableOfContents } from '../components/generated-reference.tsx';
import {
  createTypeReferenceIndex,
  genericParametersText,
  memberDeclarationText,
  shouldUseMultilineSignature,
  signatureText,
  typeDisplaySegments,
} from '../lib/generated-content/reference-rendering.ts';
import { referencePageDataSchema } from '../lib/generated-content/schemas.ts';

test('reference page table of contents lists each non-empty member group', () => {
  const page = referencePageDataSchema.parse({
    cross_references: [],
    declaration: {
      fields: [{ id: 'field:model', name: 'model', ty: { display: 'string' } }],
      id: 'interface:example.Client',
      kind: 'interface',
      name: 'Client',
      required_methods: [
        {
          id: 'method:create',
          name: 'create',
          signature: { params: [], returns: { display: 'example.Client' } },
        },
        {
          id: 'method:run',
          name: 'run',
          signature: {
            params: [{ name: 'self', ty: { display: 'example.Client' } }],
            returns: { display: 'string' },
          },
        },
      ],
    },
    display_name: 'Client',
    exported_id: 'interface:example.Client',
    implementations: [{ id: 'impl:Client', methods: [] }],
    member_anchors: [
      {
        anchor: 'model',
        exported_id: 'field:model',
        label: 'model',
        member_kind: 'field',
      },
    ],
    namespace_path: [],
    package_name: 'example',
    page_kind: 'interface',
    qualified_name: 'example.Client',
    schema_version: 1,
    summary: null,
  });

  assert.deepEqual(referencePageTableOfContents(page), [
    { href: '#signature', label: 'Signature' },
    { href: '#fields', label: 'Fields' },
    { href: '#required-static-methods', label: 'Required static methods' },
    {
      href: '#required-instance-methods',
      label: 'Required instance methods',
    },
    { href: '#implementations', label: 'Implementations' },
  ]);
});

test('signatures preserve generics and defaults and wrap only when dense', () => {
  const shortSignature = {
    generics: [{ bounds: [], name: 'Out' }],
    params: [
      { name: 'self', ty: { display: 'ai.Runner' } },
      {
        name: 'spec',
        optional: true,
        ty: { display: 'ai.FunctionSpec<Out>' },
      },
    ],
    returns: { display: 'ai.RunResult<Out>' },
  };

  assert.equal(
    signatureText('run', shortSignature),
    'run<Out>(self, spec: ai.FunctionSpec<Out> = …) -> ai.RunResult<Out>',
  );
  assert.equal(shouldUseMultilineSignature('run', shortSignature), false);
  assert.equal(
    shouldUseMultilineSignature('new', {
      params: [
        { name: 'one', ty: { display: 'string' } },
        { name: 'two', ty: { display: 'string' } },
        { name: 'three', ty: { display: 'string' } },
        { name: 'four', ty: { display: 'string' } },
      ],
      returns: { display: 'example.Client' },
    }),
    true,
  );

  const boundedSignature = {
    generics: [
      {
        bounds: ['baml.Named', 'baml.Sized'],
        name: 'T',
      },
    ],
    params: [{ name: 'value', ty: { display: 'T' } }],
    returns: { display: 'string' },
  };
  assert.equal(
    genericParametersText(boundedSignature.generics),
    '<T extends baml.Named & baml.Sized>',
  );
  assert.equal(
    signatureText('describe', boundedSignature),
    'describe<T extends baml.Named & baml.Sized>(value: T) -> string',
  );
});

test('associated type declarations include the type keyword and defaults', () => {
  assert.equal(
    memberDeclarationText(
      { id: 'assoc:Error', name: 'Error' },
      'associated-type',
    ),
    'type Error',
  );
  assert.equal(
    memberDeclarationText(
      {
        default: { display: 'baml.errors.InvalidArgument' },
        id: 'assoc:Error',
        name: 'Error',
      },
      'associated-type',
    ),
    'type Error = baml.errors.InvalidArgument',
  );
});

test('type displays link exact qualified names without partial identifiers', () => {
  const references = [
    {
      anchor: null,
      exported_id: 'class:ai.Client',
      qualified_name: 'ai.Client',
      route_path: 'ai/Client',
    },
    {
      anchor: null,
      exported_id: 'class:ai.Client.Result',
      qualified_name: 'ai.Client.Result',
      route_path: 'ai/Client/Result',
    },
  ];

  const segments = typeDisplaySegments(
    'map<ai.Client, ai.Client.Result> | notai.Client',
    createTypeReferenceIndex(references),
  );
  assert.deepEqual(
    segments.map((segment) => ({
      reference: segment.reference?.qualified_name,
      text: segment.text,
    })),
    [
      { reference: undefined, text: 'map<' },
      { reference: 'ai.Client', text: 'ai.Client' },
      { reference: undefined, text: ', ' },
      { reference: 'ai.Client.Result', text: 'ai.Client.Result' },
      { reference: undefined, text: '> | notai.Client' },
    ],
  );
});

test('type reference indexing prefers declaration routes and excludes the current page', () => {
  const index = createTypeReferenceIndex(
    [
      {
        anchor: 'run',
        exported_id: 'method:ai.Runner.run',
        qualified_name: 'ai.Runner',
        route_path: 'ai/Runner',
      },
      {
        anchor: null,
        exported_id: 'class:ai.Runner',
        qualified_name: 'ai.Runner',
        route_path: 'ai/Runner',
      },
      {
        anchor: null,
        exported_id: 'class:ai.Client',
        qualified_name: 'ai.Client',
        route_path: 'ai/Client',
      },
    ],
    'ai.Client',
  );

  assert.equal(index.get('ai.Runner')?.anchor, null);
  assert.equal(index.has('ai.Client'), false);
});
