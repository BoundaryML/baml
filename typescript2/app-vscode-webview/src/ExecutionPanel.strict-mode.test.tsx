import { StrictMode, act } from 'react';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeAll, describe, expect, it, vi } from 'vitest';
import {
  ExecutionPanel,
  type ControlFlowGraph,
  type ProjectUpdate,
  type RuntimePort,
  type Run,
  type WorkerInMessage,
  type WorkerOutMessage,
} from '@b/pkg-playground';

beforeAll(() => {
  HTMLElement.prototype.scrollTo ??= vi.fn();
});

describe('ExecutionPanel StrictMode lifecycle', () => {
  it('keeps the run-store listener alive after StrictMode effect replay', async () => {
    const port = new FakeRuntimePort();

    render(
      <StrictMode>
        <ExecutionPanel port={port} />
      </StrictMode>,
    );

    act(() => {
      port.emit({
        type: 'playgroundNotification',
        notification: {
          type: 'listProjects',
          projects: ['project'],
        },
      });
      port.emit({
        type: 'playgroundNotification',
        notification: {
          type: 'updateProject',
          project: 'project',
          update: {
            isBexCurrent: true,
            functions: [
              { name: 'ReadNote', kind: 'expr', origin: 'userDefined' },
            ],
            diagnostics: [],
          },
        },
      });
    });

    fireEvent.click(
      await screen.findByRole('button', { name: 'Functions (1)' }),
    );
    fireEvent.click(await screen.findByRole('button', { name: 'ReadNote' }));
    fireEvent.click(await screen.findByRole('button', { name: 'Run' }));

    await waitFor(() => {
      expect(port.sent.some((msg) => msg.type === 'startRun')).toBe(true);
    });

    const startRun = port.sent.find(
      (msg): msg is Extract<WorkerInMessage, { type: 'startRun' }> =>
        msg.type === 'startRun',
    );
    expect(startRun).toBeDefined();

    const run = runFixture(startRun!.project, startRun!.functionName);
    act(() => {
      port.emit({
        type: 'runStarted',
        requestId: startRun!.requestId,
        run,
      });
    });

    await waitFor(() => {
      expect(port.sent.some((msg) => msg.type === 'snapshot')).toBe(true);
    });

    const snapshot = port.sent.find(
      (msg): msg is Extract<WorkerInMessage, { type: 'snapshot' }> =>
        msg.type === 'snapshot',
    );
    expect(snapshot).toBeDefined();

    act(() => {
      port.emit({
        type: 'runSnapshot',
        requestId: snapshot!.requestId,
        boundaryId: run.boundaryId,
        snapshot: { ...run, status: 'running', cursor: 1 },
      });
    });

    expect(await screen.findByText('running...')).toBeInTheDocument();
    expect(
      screen.queryByText('Press Run to execute ReadNote()'),
    ).not.toBeInTheDocument();
  });

  it('runs the exact namespaced sidebar leaf instead of promoting it to a caller', async () => {
    const port = new FakeRuntimePort();

    render(<ExecutionPanel port={port} />);

    act(() => {
      port.emit({
        type: 'playgroundNotification',
        notification: {
          type: 'listProjects',
          projects: ['project'],
        },
      });
      port.emit({
        type: 'playgroundNotification',
        notification: {
          type: 'updateProject',
          project: 'project',
          update: {
            isBexCurrent: true,
            functions: [
              { name: 'paulo.Hi', kind: 'expr', origin: 'userDefined' },
              {
                name: 'paulo.childFunc1',
                kind: 'expr',
                origin: 'userDefined',
              },
            ],
            diagnostics: [],
          },
        },
      });
    });

    await waitFor(() => {
      expect(
        port.sent.filter((msg) => msg.type === 'requestControlFlowGraph'),
      ).toHaveLength(2);
    });

    act(() => {
      port.emit({
        type: 'playgroundNotification',
        notification: {
          type: 'controlFlowGraphResult',
          functionName: 'paulo.Hi',
          graph: graphFixture('paulo.Hi', ['childFunc1']),
        },
      });
      port.emit({
        type: 'playgroundNotification',
        notification: {
          type: 'controlFlowGraphResult',
          functionName: 'paulo.childFunc1',
          graph: graphFixture('paulo.childFunc1'),
        },
      });
    });

    fireEvent.click(
      await screen.findByRole('button', { name: 'Functions (2)' }),
    );
    fireEvent.click(await screen.findByRole('button', { name: 'paulo (2)' }));
    fireEvent.click(await screen.findByRole('button', { name: 'childFunc1' }));
    fireEvent.click(await screen.findByRole('button', { name: 'Run' }));

    await waitFor(() => {
      expect(
        port.sent.some(
          (msg) =>
            msg.type === 'startRun' &&
            msg.functionName === 'paulo.childFunc1',
        ),
      ).toBe(true);
    });
  });
});

describe('ExecutionPanel test previews', () => {
  it('hydrates legacy test args without running and releases selection on navigation', async () => {
    const port = new FakeRuntimePort();
    const projectUpdate: ProjectUpdate = {
      isBexCurrent: true,
      functions: [
        {
          name: 'ClassifySentiment',
          kind: 'llm',
          origin: 'userDefined',
          capabilities: {
            renderPrompt: true,
            buildRequest: true,
            clientName: 'Gpt5',
          },
          params: [
            {
              name: 'text',
              hasDefault: false,
              schema: { type: 'string' },
            },
          ],
        },
        { name: 'OtherFunction', kind: 'expr', origin: 'userDefined' },
      ],
      tests: [
        {
          name: 'HappySentiment',
          functionName: 'ClassifySentiment',
          argsJson: '{"text":"I absolutely love this feature"}',
        },
      ],
      diagnostics: [],
    };

    render(<ExecutionPanel port={port} />);

    act(() => {
      port.emit({
        type: 'playgroundNotification',
        notification: { type: 'listProjects', projects: ['project'] },
      });
      port.emit({
        type: 'playgroundNotification',
        notification: {
          type: 'updateProject',
          project: 'project',
          update: projectUpdate,
        },
      });
    });

    expect(
      await screen.findByRole('button', { name: 'ClassifySentiment (1)' }),
    ).toBeInTheDocument();
    fireEvent.click(
      await screen.findByTitle(
        'Use HappySentiment args for ClassifySentiment',
      ),
    );
    fireEvent.click(await screen.findByRole('button', { name: 'raw' }));

    const rawInput = await screen.findByPlaceholderText('{"key": "value"}');
    expect(JSON.parse((rawInput as HTMLInputElement).value)).toEqual({
      text: 'I absolutely love this feature',
    });
    expect(
      screen.getByText('ClassifySentiment()'),
    ).toBeInTheDocument();
    expect(port.sent.some((message) => message.type === 'startRun')).toBe(false);
    expect(port.sent.some((message) => message.type === 'startTestRun')).toBe(false);

    act(() => {
      port.emit({
        type: 'cursorContext',
        context: {
          functionName: 'OtherFunction',
          isWorkflow: false,
          workflowMemberships: [],
          sourceExprId: null,
          testName: null,
        },
      });
    });
    expect(await screen.findByText('OtherFunction()')).toBeInTheDocument();

    act(() => {
      port.emit({
        type: 'playgroundNotification',
        notification: {
          type: 'updateProject',
          project: 'project',
          update: {
            ...projectUpdate,
            tests: projectUpdate.tests?.map((test) => ({
              ...test,
              argsJson: '{"text":"source edit"}',
            })),
          },
        },
      });
    });
    expect(await screen.findByText('OtherFunction()')).toBeInTheDocument();
  });
});

describe('ExecutionPanel args form', () => {
  it('renders the args form from param schemas and serializes edits into argsJson', async () => {
    const port = new FakeRuntimePort();

    render(<ExecutionPanel port={port} />);

    act(() => {
      port.emit({
        type: 'playgroundNotification',
        notification: { type: 'listProjects', projects: ['project'] },
      });
      port.emit({
        type: 'playgroundNotification',
        notification: {
          type: 'updateProject',
          project: 'project',
          update: {
            isBexCurrent: true,
            functions: [
              {
                name: 'Greet',
                kind: 'expr',
                origin: 'userDefined',
                params: [
                  { name: 'name', hasDefault: false, schema: { type: 'string' } },
                  {
                    name: 'color',
                    hasDefault: false,
                    schema: { type: 'ref', name: 'user.Color' },
                  },
                ],
              },
            ],
            types: {
              'user.Color': {
                kind: 'enum',
                values: ['Red', 'Green', 'Blue'],
              },
            },
            diagnostics: [],
          },
        },
      });
    });

    fireEvent.click(
      await screen.findByRole('button', { name: 'Functions (1)' }),
    );
    fireEvent.click(await screen.findByRole('button', { name: 'Greet' }));

    // The form renders one widget per param: a string input and enum chips.
    const nameInput = await screen.findByPlaceholderText('text');
    fireEvent.change(nameInput, { target: { value: 'Ada' } });
    fireEvent.click(await screen.findByRole('button', { name: 'Green' }));

    // Raw view shows the same argsJson the form writes — including the enum
    // wire marker — proving the form and raw input share one state.
    fireEvent.click(await screen.findByRole('button', { name: 'raw' }));
    const rawInput = await screen.findByPlaceholderText('{"key": "value"}');
    const parsed = JSON.parse((rawInput as HTMLInputElement).value);
    expect(parsed).toEqual({
      name: 'Ada',
      color: { $baml: { enum: 'user.Color', value: 'Green' } },
    });

    fireEvent.click(await screen.findByRole('button', { name: 'Run' }));
    await waitFor(() => {
      expect(
        port.sent.some(
          (msg) => msg.type === 'startRun' && msg.functionName === 'Greet',
        ),
      ).toBe(true);
    });

    // Cmd/Ctrl+Enter from an args editor field runs with the current args.
    // (The form was toggled to raw above, so target the raw input.)
    const runCount = port.sent.filter((msg) => msg.type === 'startRun').length;
    fireEvent.keyDown(rawInput, { key: 'Enter', metaKey: true });
    await waitFor(() => {
      expect(port.sent.filter((msg) => msg.type === 'startRun').length).toBe(
        runCount + 1,
      );
    });
  });

  it('preserves seeded defaults when switching functions and back', async () => {
    const port = new FakeRuntimePort();

    render(<ExecutionPanel port={port} />);

    act(() => {
      port.emit({
        type: 'playgroundNotification',
        notification: { type: 'listProjects', projects: ['project'] },
      });
      port.emit({
        type: 'playgroundNotification',
        notification: {
          type: 'updateProject',
          project: 'project',
          update: {
            isBexCurrent: true,
            functions: [
              {
                name: 'Greet',
                kind: 'expr',
                origin: 'userDefined',
                params: [
                  {
                    name: 'color',
                    hasDefault: false,
                    schema: { type: 'ref', name: 'user.Color' },
                  },
                ],
              },
              { name: 'Zero', kind: 'expr', origin: 'userDefined', params: [] },
            ],
            types: {
              'user.Color': {
                kind: 'enum',
                values: ['Red', 'Green', 'Blue'],
              },
            },
            diagnostics: [],
          },
        },
      });
    });

    fireEvent.click(
      await screen.findByRole('button', { name: 'Functions (2)' }),
    );
    fireEvent.click(await screen.findByRole('button', { name: 'Greet' }));
    // Wait for the seed effect, then bounce to Zero and back.
    const seeded = {
      color: { $baml: { enum: 'user.Color', value: 'Red' } },
    };
    fireEvent.click(await screen.findByRole('button', { name: 'raw' }));
    const rawInput = await screen.findByPlaceholderText('{"key": "value"}');
    await waitFor(() => {
      expect(JSON.parse((rawInput as HTMLInputElement).value)).toEqual(seeded);
    });

    fireEvent.click(await screen.findByRole('button', { name: 'Zero' }));
    fireEvent.click(await screen.findByRole('button', { name: 'Greet' }));

    const rawAgain = await screen.findByPlaceholderText('{"key": "value"}');
    await waitFor(() => {
      expect(JSON.parse((rawAgain as HTMLInputElement).value)).toEqual(seeded);
    });
  });

  it('reconciles cached form values when a same-name function schema changes', async () => {
    const port = new FakeRuntimePort();

    render(<ExecutionPanel port={port} />);

    act(() => {
      port.emit({
        type: 'playgroundNotification',
        notification: { type: 'listProjects', projects: ['project'] },
      });
      port.emit({
        type: 'playgroundNotification',
        notification: {
          type: 'updateProject',
          project: 'project',
          update: {
            isBexCurrent: true,
            functions: [
              {
                name: 'EchoPerson',
                kind: 'expr',
                origin: 'userDefined',
                params: [
                  {
                    name: 'person',
                    hasDefault: false,
                    schema: { type: 'ref', name: 'user.Person' },
                  },
                ],
              },
            ],
            types: {
              'user.Person': {
                kind: 'class',
                fields: [
                  { name: 'name', schema: { type: 'string' } },
                  { name: 'active', schema: { type: 'bool' } },
                ],
              },
            },
            diagnostics: [],
          },
        },
      });
    });

    fireEvent.click(
      await screen.findByRole('button', { name: 'Functions (1)' }),
    );
    fireEvent.click(await screen.findByRole('button', { name: 'EchoPerson' }));
    fireEvent.change(await screen.findByPlaceholderText('text'), {
      target: { value: 'Ada' },
    });

    // Hot-reload the same function name with a new required class field.
    act(() => {
      port.emit({
        type: 'playgroundNotification',
        notification: {
          type: 'updateProject',
          project: 'project',
          update: {
            isBexCurrent: true,
            functions: [
              {
                name: 'EchoPerson',
                kind: 'expr',
                origin: 'userDefined',
                params: [
                  {
                    name: 'person',
                    hasDefault: false,
                    schema: { type: 'ref', name: 'user.Person' },
                  },
                ],
              },
            ],
            types: {
              'user.Person': {
                kind: 'class',
                fields: [
                  { name: 'name', schema: { type: 'string' } },
                  { name: 'active', schema: { type: 'bool' } },
                  { name: 'age', schema: { type: 'int' } },
                ],
              },
            },
            diagnostics: [],
          },
        },
      });
    });

    const ageInput = await screen.findByPlaceholderText('0');
    await waitFor(() => {
      expect(ageInput).toHaveValue('0');
      expect(screen.getByPlaceholderText('text')).toHaveValue('Ada');
    });
    fireEvent.change(screen.getByPlaceholderText('text'), {
      target: { value: 'Ada Lovelace' },
    });

    fireEvent.click(await screen.findByRole('button', { name: 'raw' }));
    const rawInput = await screen.findByPlaceholderText('{"key": "value"}');
    await waitFor(() => {
      expect(JSON.parse((rawInput as HTMLInputElement).value)).toEqual({
        person: {
          $baml: { type: 'user.Person' },
          name: 'Ada Lovelace',
          active: false,
          age: 0,
        },
      });
    });
  });

  it('serializes untouched defaults inside a required nested class', async () => {
    const port = new FakeRuntimePort();

    render(<ExecutionPanel port={port} />);

    act(() => {
      port.emit({
        type: 'playgroundNotification',
        notification: { type: 'listProjects', projects: ['project'] },
      });
      port.emit({
        type: 'playgroundNotification',
        notification: {
          type: 'updateProject',
          project: 'project',
          update: {
            isBexCurrent: true,
            functions: [
              {
                name: 'EchoEnvelope',
                kind: 'expr',
                origin: 'userDefined',
                params: [
                  {
                    name: 'envelope',
                    hasDefault: false,
                    schema: { type: 'ref', name: 'user.Envelope' },
                  },
                ],
              },
            ],
            types: {
              'user.Flag': {
                kind: 'class',
                fields: [{ name: 'active', schema: { type: 'bool' } }],
              },
              'user.Envelope': {
                kind: 'class',
                fields: [
                  {
                    name: 'flag',
                    schema: { type: 'ref', name: 'user.Flag' },
                  },
                ],
              },
            },
            diagnostics: [],
          },
        },
      });
    });

    fireEvent.click(
      await screen.findByRole('button', { name: 'Functions (1)' }),
    );
    fireEvent.click(
      await screen.findByRole('button', { name: 'EchoEnvelope' }),
    );

    // The switch is untouched and visually false; raw state must already
    // contain that same false value rather than a marker-only nested class.
    expect(await screen.findByRole('switch')).not.toBeChecked();
    fireEvent.click(await screen.findByRole('button', { name: 'raw' }));
    const rawInput = await screen.findByPlaceholderText('{"key": "value"}');
    await waitFor(() => {
      expect(JSON.parse((rawInput as HTMLInputElement).value)).toEqual({
        envelope: {
          $baml: { type: 'user.Envelope' },
          flag: {
            $baml: { type: 'user.Flag' },
            active: false,
          },
        },
      });
    });
  });

  it('keeps an explicitly chosen union variant selected when values overlap', async () => {
    const port = new FakeRuntimePort();

    render(<ExecutionPanel port={port} />);

    act(() => {
      port.emit({
        type: 'playgroundNotification',
        notification: { type: 'listProjects', projects: ['project'] },
      });
      port.emit({
        type: 'playgroundNotification',
        notification: {
          type: 'updateProject',
          project: 'project',
          update: {
            isBexCurrent: true,
            functions: [
              {
                name: 'Mix',
                kind: 'expr',
                origin: 'userDefined',
                params: [
                  {
                    name: 'x',
                    hasDefault: false,
                    schema: {
                      type: 'union',
                      variants: [{ type: 'int' }, { type: 'float' }],
                    },
                  },
                ],
              },
            ],
            types: {},
            diagnostics: [],
          },
        },
      });
    });

    fireEvent.click(
      await screen.findByRole('button', { name: 'Functions (1)' }),
    );
    fireEvent.click(await screen.findByRole('button', { name: 'Mix' }));

    // Seeded to the first variant's default (int 0), which also inhabits
    // float. Picking float must stick — first-match detection alone would
    // snap the selection back to int and integer-gate the input.
    const floatChip = await screen.findByRole('button', { name: 'float' });
    fireEvent.click(floatChip);
    expect(floatChip.className).toContain('bg-vsc-accent');

    const numberInput = await screen.findByPlaceholderText('0.0');
    fireEvent.change(numberInput, { target: { value: '1.5' } });
    expect(floatChip.className).toContain('bg-vsc-accent');

    fireEvent.click(await screen.findByRole('button', { name: 'raw' }));
    const rawInput = await screen.findByPlaceholderText('{"key": "value"}');
    expect(JSON.parse((rawInput as HTMLInputElement).value)).toEqual({
      x: 1.5,
    });
  });

  it('renders a recursive class fully typed at each expanded depth', async () => {
    const port = new FakeRuntimePort();

    render(<ExecutionPanel port={port} />);

    act(() => {
      port.emit({
        type: 'playgroundNotification',
        notification: { type: 'listProjects', projects: ['project'] },
      });
      port.emit({
        type: 'playgroundNotification',
        notification: {
          type: 'updateProject',
          project: 'project',
          update: {
            isBexCurrent: true,
            functions: [
              {
                name: 'Walk',
                kind: 'expr',
                origin: 'userDefined',
                params: [
                  {
                    name: 't',
                    hasDefault: false,
                    schema: { type: 'ref', name: 'user.Tree' },
                  },
                ],
              },
            ],
            types: {
              'user.Tree': {
                kind: 'class',
                fields: [
                  { name: 'value', schema: { type: 'int' } },
                  {
                    name: 'children',
                    schema: {
                      type: 'list',
                      item: { type: 'ref', name: 'user.Tree' },
                    },
                  },
                ],
              },
            },
            diagnostics: [],
          },
        },
      });
    });

    fireEvent.click(
      await screen.findByRole('button', { name: 'Functions (1)' }),
    );
    fireEvent.click(await screen.findByRole('button', { name: 'Walk' }));

    // Root Tree section is open; one typed int widget for `value`.
    expect(await screen.findAllByPlaceholderText('0')).toHaveLength(1);

    // Adding a child materializes a nested Tree section. It starts collapsed
    // (depth ≥ 2) with its content unmounted — expanding it resolves the ref
    // lazily and renders typed widgets: no raw-JSON cut-point at any depth.
    fireEvent.click(await screen.findByRole('button', { name: 'add item' }));
    const treeSections = await screen.findAllByRole('button', {
      name: 'Tree',
    });
    expect(treeSections).toHaveLength(2);
    expect(screen.getAllByPlaceholderText('0')).toHaveLength(1);
    fireEvent.click(treeSections[1]);
    await waitFor(() => {
      expect(screen.getAllByPlaceholderText('0')).toHaveLength(2);
    });

    fireEvent.change(screen.getAllByPlaceholderText('0')[1], {
      target: { value: '7' },
    });
    fireEvent.click(await screen.findByRole('button', { name: 'raw' }));
    const rawInput = await screen.findByPlaceholderText('{"key": "value"}');
    expect(JSON.parse((rawInput as HTMLInputElement).value)).toEqual({
      t: {
        $baml: { type: 'user.Tree' },
        value: 0,
        children: [
          { $baml: { type: 'user.Tree' }, value: 7, children: [] },
        ],
      },
    });
  });

  it('renders a self-referential alias as raw JSON instead of recursing', async () => {
    // `type A = A | int` compiles clean and produces a table entry whose
    // schema contains a ref back to itself with no value boundary in
    // between. The render path must cut the cycle (raw-JSON fallback for the
    // re-entrant ref), not stack-overflow.
    const port = new FakeRuntimePort();

    render(<ExecutionPanel port={port} />);

    act(() => {
      port.emit({
        type: 'playgroundNotification',
        notification: { type: 'listProjects', projects: ['project'] },
      });
      port.emit({
        type: 'playgroundNotification',
        notification: {
          type: 'updateProject',
          project: 'project',
          update: {
            isBexCurrent: true,
            functions: [
              {
                name: 'Loop',
                kind: 'expr',
                origin: 'userDefined',
                params: [
                  {
                    name: 'a',
                    hasDefault: false,
                    schema: { type: 'ref', name: 'user.A' },
                  },
                ],
              },
            ],
            types: {
              'user.A': {
                kind: 'alias',
                schema: {
                  type: 'union',
                  variants: [
                    { type: 'ref', name: 'user.A' },
                    { type: 'int' },
                  ],
                },
              },
            },
            diagnostics: [],
          },
        },
      });
    });

    fireEvent.click(
      await screen.findByRole('button', { name: 'Functions (1)' }),
    );
    fireEvent.click(await screen.findByRole('button', { name: 'Loop' }));

    // Union chips render; the self-referential variant is reachable but its
    // widget degrades to the raw-JSON textarea.
    fireEvent.click(await screen.findByRole('button', { name: 'A' }));
    expect(
      await screen.findByPlaceholderText('JSON (A)'),
    ).toBeInTheDocument();
  });

  it('normalizes a bare-enum host seed into the wire marker', async () => {
    const port = new FakeRuntimePort();

    render(<ExecutionPanel port={port} initialArgsJson='{"c":"Red"}' />);

    act(() => {
      port.emit({
        type: 'playgroundNotification',
        notification: { type: 'listProjects', projects: ['project'] },
      });
      port.emit({
        type: 'playgroundNotification',
        notification: {
          type: 'updateProject',
          project: 'project',
          update: {
            isBexCurrent: true,
            functions: [
              {
                name: 'IsRed',
                kind: 'expr',
                origin: 'userDefined',
                params: [
                  {
                    name: 'c',
                    hasDefault: false,
                    schema: { type: 'ref', name: 'user.Color' },
                  },
                ],
              },
            ],
            types: {
              'user.Color': {
                kind: 'enum',
                values: ['Red', 'Green', 'Blue'],
              },
            },
            diagnostics: [],
          },
        },
      });
    });

    fireEvent.click(
      await screen.findByRole('button', { name: 'Functions (1)' }),
    );
    fireEvent.click(await screen.findByRole('button', { name: 'IsRed' }));

    // The bare string would encode untyped (no String→Enum coercion exists);
    // hydration must rewrite it to the marker even though the host seed is
    // non-empty (so the seeding effect correctly stays away).
    fireEvent.click(await screen.findByRole('button', { name: 'raw' }));
    const rawInput = await screen.findByPlaceholderText('{"key": "value"}');
    await waitFor(() => {
      expect(JSON.parse((rawInput as HTMLInputElement).value)).toEqual({
        c: { $baml: { enum: 'user.Color', value: 'Red' } },
      });
    });
  });

  it('shows the no-arguments state for a nullary schema and still runs', async () => {
    const port = new FakeRuntimePort();

    render(<ExecutionPanel port={port} />);

    act(() => {
      port.emit({
        type: 'playgroundNotification',
        notification: { type: 'listProjects', projects: ['project'] },
      });
      port.emit({
        type: 'playgroundNotification',
        notification: {
          type: 'updateProject',
          project: 'project',
          update: {
            isBexCurrent: true,
            functions: [
              { name: 'Zero', kind: 'expr', origin: 'userDefined', params: [] },
            ],
            diagnostics: [],
          },
        },
      });
    });

    fireEvent.click(
      await screen.findByRole('button', { name: 'Functions (1)' }),
    );
    fireEvent.click(await screen.findByRole('button', { name: 'Zero' }));

    expect(
      await screen.findByText('This function takes no arguments.'),
    ).toBeInTheDocument();

    fireEvent.click(await screen.findByRole('button', { name: 'Run' }));
    await waitFor(() => {
      expect(
        port.sent.some(
          (msg) => msg.type === 'startRun' && msg.functionName === 'Zero',
        ),
      ).toBe(true);
    });
  });
});

class FakeRuntimePort implements RuntimePort {
  sent: WorkerInMessage[] = [];
  private handlers = new Set<(msg: WorkerOutMessage) => void>();

  postMessage(msg: WorkerInMessage): void {
    this.sent.push(msg);
  }

  onMessage(handler: (msg: WorkerOutMessage) => void): () => void {
    this.handlers.add(handler);
    return () => {
      this.handlers.delete(handler);
    };
  }

  emit(msg: WorkerOutMessage): void {
    for (const handler of this.handlers) {
      handler(msg);
    }
  }

  dispose(): void {
    this.handlers.clear();
  }
}

function runFixture(projectId: string, functionName: string): Run {
  return {
    boundaryId: 'baml_id_1_AAAAAAAAAAAAAAAAAAAAAQ',
    target: { kind: 'function', functionName },
    visibility: { kind: 'history' },
    status: 'pending',
    createdAtMs: 100,
    startedAtMs: null,
    completedAtMs: null,
    timeAnchor: {
      epochCreatedAtMs: 100,
      traceZeroNs: '0',
    },
    request: {
      projectId,
      projectGeneration: 1,
      target: { kind: 'function', functionName },
      argsSummary: '{}',
      optionsSummary: null,
    },
    result: null,
    error: null,
    cancellation: null,
    rootCallNodeId: null,
    graphRuntimeOverlay: null,
    calls: [],
    threads: [],
    payloads: [],
    diagnostics: [],
    cursor: 0,
  };
}

function graphFixture(
  functionName: string,
  calleeNames: string[] = [],
): ControlFlowGraph {
  return {
    nodes: {
      '1': {
        id: 1,
        parentNodeId: null,
        logFilterKey: functionName,
        label: functionName,
        sourceExpr: null,
        nodeType: 'functionRoot',
        calleeNames,
        isContainer: true,
      },
    },
    edgesBySrc: {},
  };
}
