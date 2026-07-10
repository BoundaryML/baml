import { StrictMode, act } from 'react';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeAll, describe, expect, it, vi } from 'vitest';
import {
  ExecutionPanel,
  type ControlFlowGraph,
  type RuntimePort,
  type Run,
  type WorkerInMessage,
  type WorkerOutMessage,
} from '@b/pkg-playground';

beforeAll(() => {
  HTMLElement.prototype.scrollTo ??= vi.fn();
  globalThis.ResizeObserver ??= class ResizeObserver {
    observe() {}
    unobserve() {}
    disconnect() {}
  };
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

describe('ExecutionPanel demand-gated project runtime', () => {
  it('reacquires the selected lease after a transport session reset', async () => {
    const port = new FakeRuntimePort();
    render(<ExecutionPanel port={port} />);
    const catalog = (sessionEpoch: number): WorkerOutMessage => ({
      type: 'playgroundNotification',
      notification: {
        type: 'listProjects',
        sessionEpoch,
        projects: ['/project'],
        entries: [
          { project: '/project', incarnation: 1, sourceRevision: 1 },
        ],
      },
    });

    act(() => {
      port.emit({ type: 'runtimeSessionReset', sessionEpoch: 1 });
      port.emit(catalog(1));
    });
    await waitFor(() => {
      expect(
        port.sent.filter((message) => message.type === 'ensureProjectRuntime'),
      ).toHaveLength(1);
    });

    act(() => {
      port.emit({ type: 'runtimeSessionReset', sessionEpoch: 2 });
      port.emit(catalog(2));
    });

    await waitFor(() => {
      expect(
        port.sent.filter((message) => message.type === 'ensureProjectRuntime'),
      ).toHaveLength(2);
    });
  });

  it('retains the last test tree and shows a qualified collection error', async () => {
    const port = new FakeRuntimePort();
    render(<ExecutionPanel port={port} />);
    const treeBytes = Array.from(
      new TextEncoder().encode(
        JSON.stringify([{ type: 'test', name: 'suite/kept' }]),
      ),
    );

    act(() => {
      port.emit({ type: 'runtimeSessionReset', sessionEpoch: 9 });
      port.emit({
        type: 'playgroundNotification',
        notification: {
          type: 'listProjects',
          sessionEpoch: 9,
          projects: ['/project'],
          entries: [
            { project: '/project', incarnation: 2, sourceRevision: 8 },
          ],
        },
      });
      port.emit({
        type: 'playgroundNotification',
        notification: {
          type: 'updateProject',
          sessionEpoch: 9,
          project: '/project',
          update: {
            sourceRevision: 8,
            projectIncarnation: 2,
            runtime: {
              state: 'ready',
              requestedRevision: 8,
              installedRevision: 8,
              generation: 4,
              hasLastKnownGood: true,
            },
            isBexCurrent: true,
            functions: [],
            diagnostics: [],
          },
        },
      });
    });

    expect(await screen.findByTitle('Re-collect tests')).toBeEnabled();
    act(() => {
      port.emit({
        type: 'playgroundNotification',
        notification: {
          type: 'testCollectionResult',
          sessionEpoch: 9,
          project: '/project',
          projectIncarnation: 2,
          sourceRevision: 8,
          generation: 4,
          collectionEpoch: 1,
          callId: 10,
          data: treeBytes,
        },
      });
    });

    expect(await screen.findByText('kept')).toBeInTheDocument();

    act(() => {
      port.emit({
        type: 'playgroundNotification',
        notification: {
          type: 'testCollectionResult',
          sessionEpoch: 8,
          project: '/project',
          projectIncarnation: 2,
          sourceRevision: 8,
          generation: 4,
          collectionEpoch: 2,
          callId: 11,
          data: [],
          collectionError: 'stale collection error',
        },
      });
      port.emit({
        type: 'playgroundNotification',
        notification: {
          type: 'testCollectionResult',
          sessionEpoch: 9,
          project: '/project',
          projectIncarnation: 2,
          sourceRevision: 8,
          generation: 4,
          collectionEpoch: 2,
          callId: 12,
          data: [],
          collectionError: 'collector exploded',
        },
      });
    });

    expect(await screen.findByText('collector exploded')).toBeInTheDocument();
    expect(screen.queryByText('stale collection error')).not.toBeInTheDocument();
    expect(screen.getByText('kept')).toBeInTheDocument();
  });

  it('retains a source-stale test tree but disables its launches', async () => {
    const port = new FakeRuntimePort();
    render(<ExecutionPanel port={port} />);
    const treeBytes = Array.from(
      new TextEncoder().encode(
        JSON.stringify([{ type: 'test', name: 'suite/kept' }]),
      ),
    );

    act(() => {
      port.emit({ type: 'runtimeSessionReset', sessionEpoch: 10 });
      port.emit({
        type: 'playgroundNotification',
        notification: {
          type: 'listProjects',
          sessionEpoch: 10,
          projects: ['/project'],
          entries: [
            { project: '/project', incarnation: 2, sourceRevision: 8 },
          ],
        },
      });
      port.emit({
        type: 'playgroundNotification',
        notification: {
          type: 'updateProject',
          sessionEpoch: 10,
          project: '/project',
          update: {
            sourceRevision: 8,
            projectIncarnation: 2,
            runtime: {
              state: 'ready',
              requestedRevision: 8,
              installedRevision: 8,
              generation: 4,
              hasLastKnownGood: true,
            },
            isBexCurrent: true,
            functions: [],
            diagnostics: [],
          },
        },
      });
      port.emit({
        type: 'playgroundNotification',
        notification: {
          type: 'testCollectionResult',
          sessionEpoch: 10,
          project: '/project',
          projectIncarnation: 2,
          sourceRevision: 8,
          generation: 4,
          collectionEpoch: 1,
          callId: 10,
          data: treeBytes,
        },
      });
    });

    expect(await screen.findByText('kept')).toBeInTheDocument();
    expect(screen.getByTitle('Run test: suite/kept')).toBeEnabled();

    act(() => {
      port.emit({
        type: 'playgroundNotification',
        notification: {
          type: 'listProjects',
          sessionEpoch: 10,
          projects: ['/project'],
          entries: [
            { project: '/project', incarnation: 2, sourceRevision: 9 },
          ],
        },
      });
      port.emit({
        type: 'playgroundNotification',
        notification: {
          type: 'updateProject',
          sessionEpoch: 10,
          project: '/project',
          update: {
            sourceRevision: 9,
            projectIncarnation: 2,
            runtime: {
              state: 'blockedByDiagnostics',
              requestedRevision: 9,
              installedRevision: 8,
              generation: 4,
              hasLastKnownGood: true,
            },
            isBexCurrent: false,
            functions: [],
            diagnostics: [{ severity: 'error', message: 'invalid source' }],
          },
        },
      });
    });

    expect(await screen.findByText('stale')).toBeInTheDocument();
    expect(screen.getByText('kept')).toBeInTheDocument();
    expect(screen.getByTitle('Run test: suite/kept')).toBeDisabled();
  });

  it('stales a test tree when runtime inputs rebuild the same source revision', async () => {
    const port = new FakeRuntimePort();
    render(<ExecutionPanel port={port} />);
    const treeBytes = Array.from(
      new TextEncoder().encode(
        JSON.stringify([{ type: 'test', name: 'suite/current' }]),
      ),
    );
    const emitRuntime = (
      state: 'building' | 'ready',
      generation: number,
    ) => {
      port.emit({
        type: 'playgroundNotification',
        notification: {
          type: 'updateProject',
          sessionEpoch: 11,
          project: '/project',
          update: {
            sourceRevision: 8,
            projectIncarnation: 2,
            runtime: {
              state,
              requestedRevision: 8,
              installedRevision: state === 'ready' ? 8 : null,
              generation,
              hasLastKnownGood: true,
            },
            isBexCurrent: state === 'ready',
            functions: [],
            diagnostics: [],
          },
        },
      });
    };
    const emitCollection = (generation: number, collectionEpoch: number) => {
      port.emit({
        type: 'playgroundNotification',
        notification: {
          type: 'testCollectionResult',
          sessionEpoch: 11,
          project: '/project',
          projectIncarnation: 2,
          sourceRevision: 8,
          generation,
          collectionEpoch,
          callId: collectionEpoch,
          data: treeBytes,
        },
      });
    };

    act(() => {
      port.emit({ type: 'runtimeSessionReset', sessionEpoch: 11 });
      port.emit({
        type: 'playgroundNotification',
        notification: {
          type: 'listProjects',
          sessionEpoch: 11,
          projects: ['/project'],
          entries: [
            { project: '/project', incarnation: 2, sourceRevision: 8 },
          ],
        },
      });
      emitRuntime('ready', 4);
      emitCollection(4, 1);
    });

    expect(await screen.findByTitle('Run test: suite/current')).toBeEnabled();

    act(() => emitRuntime('building', 4));
    expect(await screen.findByText('stale')).toBeInTheDocument();
    expect(screen.getByTitle('Run test: suite/current')).toBeDisabled();

    act(() => emitRuntime('ready', 5));
    expect(screen.getByTitle('Run test: suite/current')).toBeDisabled();

    act(() => emitCollection(5, 2));
    await waitFor(() => {
      expect(screen.queryByText('stale')).not.toBeInTheDocument();
      expect(screen.getByTitle('Run test: suite/current')).toBeEnabled();
    });
  });

  it('fences CFG responses by session, generation, and derived epoch', async () => {
    const port = new FakeRuntimePort();
    render(<ExecutionPanel port={port} />);

    act(() => {
      port.emit({ type: 'runtimeSessionReset', sessionEpoch: 5 });
      port.emit({
        type: 'playgroundNotification',
        notification: {
          type: 'listProjects',
          sessionEpoch: 5,
          projects: ['/project'],
          entries: [
            { project: '/project', incarnation: 3, sourceRevision: 12 },
          ],
        },
      });
      port.emit({
        type: 'playgroundNotification',
        notification: {
          type: 'updateProject',
          sessionEpoch: 5,
          project: '/project',
          update: {
            sourceRevision: 12,
            projectIncarnation: 3,
            runtime: {
              state: 'ready',
              requestedRevision: 12,
              installedRevision: 12,
              generation: 4,
              hasLastKnownGood: true,
            },
            isBexCurrent: true,
            functions: [
              { name: 'Workflow', kind: 'expr', origin: 'userDefined' },
            ],
            diagnostics: [],
          },
        },
      });
    });

    fireEvent.click(
      await screen.findByRole('button', { name: 'Functions (1)' }),
    );
    fireEvent.click(await screen.findByRole('button', { name: 'Workflow' }));
    await waitFor(() => {
      expect(
        port.sent.some(
          (message) =>
            message.type === 'requestControlFlowGraph' &&
            message.functionName === 'Workflow',
        ),
      ).toBe(true);
    });

    act(() => {
      port.emit({
        type: 'controlFlowGraphResult',
        sessionEpoch: 4,
        project: '/project',
        projectIncarnation: 3,
        sourceRevision: 12,
        generation: 4,
        derivedEpoch: 2,
        functionName: 'Workflow',
        graph: graphFixture('WrongSession'),
      });
      port.emit({
        type: 'controlFlowGraphResult',
        sessionEpoch: 5,
        project: '/project',
        projectIncarnation: 3,
        sourceRevision: 12,
        generation: 3,
        derivedEpoch: 2,
        functionName: 'Workflow',
        graph: graphFixture('WrongGeneration'),
      });
    });

    expect(screen.queryByText('WrongSession')).not.toBeInTheDocument();
    expect(screen.queryByText('WrongGeneration')).not.toBeInTheDocument();

    act(() => {
      port.emit({
        type: 'controlFlowGraphResult',
        sessionEpoch: 5,
        project: '/project',
        projectIncarnation: 3,
        sourceRevision: 12,
        generation: 4,
        derivedEpoch: 2,
        functionName: 'Workflow',
        graph: graphFixture('CurrentGraph'),
      });
    });
    expect(await screen.findByText('CurrentGraph')).toBeInTheDocument();

    act(() => {
      port.emit({
        type: 'controlFlowGraphResult',
        sessionEpoch: 5,
        project: '/project',
        projectIncarnation: 3,
        sourceRevision: 12,
        generation: 4,
        derivedEpoch: 1,
        functionName: 'Workflow',
        graph: graphFixture('RegressiveGraph'),
      });
    });
    expect(screen.queryByText('RegressiveGraph')).not.toBeInTheDocument();
    expect(screen.getByText('CurrentGraph')).toBeInTheDocument();
  });

  it('moves one selected-project lease instead of warming the catalog', async () => {
    const port = new FakeRuntimePort();
    render(<ExecutionPanel port={port} />);

    act(() => {
      port.emit({
        type: 'playgroundNotification',
        notification: {
          type: 'listProjects',
          projects: ['/a', '/b'],
          entries: [
            { project: '/a', incarnation: 1, sourceRevision: 1 },
            { project: '/b', incarnation: 4, sourceRevision: 9 },
          ],
        },
      });
    });

    await waitFor(() => {
      expect(
        port.sent.filter((message) => message.type === 'ensureProjectRuntime'),
      ).toHaveLength(1);
    });
    expect(
      port.sent.find((message) => message.type === 'ensureProjectRuntime'),
    ).toMatchObject({ project: '/a', incarnation: 1 });

    fireEvent.click(await screen.findByRole('button', { name: '/b' }));

    await waitFor(() => {
      const leaseMessages = port.sent.filter(
        (message) =>
          message.type === 'ensureProjectRuntime' ||
          message.type === 'releaseProjectRuntime',
      );
      expect(leaseMessages).toHaveLength(3);
      expect(leaseMessages.slice(-2)).toEqual([
        {
          type: 'releaseProjectRuntime',
          requestId: expect.any(Number),
          project: '/a',
          incarnation: 1,
        },
        {
          type: 'ensureProjectRuntime',
          requestId: expect.any(Number),
          project: '/b',
          incarnation: 4,
        },
      ]);
    });
  });

  it('clears project-scoped selection before rendering the next project', async () => {
    const port = new FakeRuntimePort();
    render(<ExecutionPanel port={port} />);

    const readyUpdate = (
      project: string,
      incarnation: number,
      sourceRevision: number,
      functionName: string,
    ): WorkerOutMessage => ({
      type: 'playgroundNotification',
      notification: {
        type: 'updateProject',
        project,
        update: {
          sourceRevision,
          projectIncarnation: incarnation,
          runtime: {
            state: 'ready',
            requestedRevision: sourceRevision,
            installedRevision: sourceRevision,
            generation: incarnation,
            hasLastKnownGood: true,
          },
          isBexCurrent: true,
          functions: [
            { name: functionName, kind: 'expr', origin: 'userDefined' },
          ],
          diagnostics: [],
        },
      },
    });

    act(() => {
      port.emit({
        type: 'playgroundNotification',
        notification: {
          type: 'listProjects',
          projects: ['/a', '/b'],
          entries: [
            { project: '/a', incarnation: 1, sourceRevision: 1 },
            { project: '/b', incarnation: 2, sourceRevision: 3 },
          ],
        },
      });
      port.emit(readyUpdate('/a', 1, 1, 'FunctionA'));
      port.emit(readyUpdate('/b', 2, 3, 'FunctionB'));
    });

    fireEvent.click(
      await screen.findByRole('button', { name: 'Functions (1)' }),
    );
    fireEvent.click(await screen.findByRole('button', { name: 'FunctionA' }));
    expect(
      await screen.findByText('Press Run to execute FunctionA()'),
    ).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '/b' }));

    expect(
      screen.queryByText('Press Run to execute FunctionA()'),
    ).not.toBeInTheDocument();
    expect(await screen.findByRole('button', { name: 'FunctionB' })).toBeInTheDocument();
  });

  it('renders preparation immediately and rejects stale project payloads', async () => {
    const port = new FakeRuntimePort();
    render(<ExecutionPanel port={port} />);

    act(() => {
      port.emit({
        type: 'playgroundNotification',
        notification: {
          type: 'listProjects',
          projects: ['/project'],
          entries: [
            { project: '/project', incarnation: 2, sourceRevision: 8 },
          ],
        },
      });
    });

    expect(await screen.findByText('Preparing current build…')).toBeInTheDocument();
    await waitFor(() => {
      expect(port.sent).toContainEqual({
        type: 'ensureProjectRuntime',
        requestId: expect.any(Number),
        project: '/project',
        incarnation: 2,
      });
    });
    expect(screen.getByTitle('Re-collect tests')).toBeDisabled();

    act(() => {
      port.emit({
        type: 'playgroundNotification',
        notification: {
          type: 'updateProject',
          project: '/project',
          update: {
            sourceRevision: 7,
            projectIncarnation: 2,
            runtime: {
              state: 'ready',
              requestedRevision: 7,
              installedRevision: 7,
              hasLastKnownGood: true,
            },
            isBexCurrent: true,
            functions: [
              { name: 'StaleFunction', kind: 'expr', origin: 'userDefined' },
            ],
            diagnostics: [],
          },
        },
      });
    });
    expect(screen.queryByText('StaleFunction')).not.toBeInTheDocument();

    act(() => {
      port.emit({
        type: 'playgroundNotification',
        notification: {
          type: 'updateProject',
          project: '/project',
          update: {
            sourceRevision: 8,
            projectIncarnation: 2,
            runtime: {
              state: 'ready',
              requestedRevision: 8,
              installedRevision: 8,
              generation: 4,
              hasLastKnownGood: true,
            },
            isBexCurrent: true,
            functions: [
              { name: 'CurrentFunction', kind: 'expr', origin: 'userDefined' },
            ],
            diagnostics: [],
          },
        },
      });
    });

    fireEvent.click(
      await screen.findByRole('button', { name: 'Functions (1)' }),
    );
    expect(await screen.findByRole('button', { name: 'CurrentFunction' })).toBeInTheDocument();
    expect(screen.getByTitle('Re-collect tests')).toBeEnabled();
  });

  it('purges cached state when a project is removed and re-added', async () => {
    const port = new FakeRuntimePort();
    render(<ExecutionPanel port={port} />);

    const emitCatalog = (incarnation: number, sourceRevision: number) => {
      port.emit({
        type: 'playgroundNotification',
        notification: {
          type: 'listProjects',
          projects: ['/project'],
          entries: [{ project: '/project', incarnation, sourceRevision }],
        },
      });
    };
    const emitReady = (incarnation: number, sourceRevision: number, name: string) => {
      port.emit({
        type: 'playgroundNotification',
        notification: {
          type: 'updateProject',
          project: '/project',
          update: {
            sourceRevision,
            projectIncarnation: incarnation,
            runtime: {
              state: 'ready',
              requestedRevision: sourceRevision,
              installedRevision: sourceRevision,
              generation: incarnation,
              hasLastKnownGood: true,
            },
            isBexCurrent: true,
            functions: [{ name, kind: 'expr', origin: 'userDefined' }],
            diagnostics: [],
          },
        },
      });
    };

    act(() => {
      emitCatalog(1, 3);
      emitReady(1, 3, 'OldFunction');
    });
    fireEvent.click(
      await screen.findByRole('button', { name: 'Functions (1)' }),
    );
    expect(await screen.findByRole('button', { name: 'OldFunction' })).toBeInTheDocument();

    act(() => {
      port.emit({
        type: 'playgroundNotification',
        notification: { type: 'listProjects', projects: [], entries: [] },
      });
      emitCatalog(2, 1);
      emitReady(1, 99, 'LateOldFunction');
    });

    expect(await screen.findByText('Preparing current build…')).toBeInTheDocument();
    expect(screen.queryByText('OldFunction')).not.toBeInTheDocument();
    expect(screen.queryByText('LateOldFunction')).not.toBeInTheDocument();

    act(() => emitReady(2, 1, 'NewFunction'));
    expect(await screen.findByRole('button', { name: 'NewFunction' })).toBeInTheDocument();
  });

  it('retries a terminal build only after the explicit Retry build action', async () => {
    const port = new FakeRuntimePort();
    render(<ExecutionPanel port={port} />);

    act(() => {
      port.emit({
        type: 'playgroundNotification',
        notification: {
          type: 'listProjects',
          projects: ['/project'],
          entries: [
            { project: '/project', incarnation: 1, sourceRevision: 5 },
          ],
        },
      });
      port.emit({
        type: 'playgroundNotification',
        notification: {
          type: 'updateProject',
          project: '/project',
          update: {
            sourceRevision: 5,
            projectIncarnation: 1,
            runtime: {
              state: 'failed',
              requestedRevision: 5,
              installedRevision: 4,
              hasLastKnownGood: true,
              error: '$init failed',
            },
            isBexCurrent: false,
            functions: [
              { name: 'LastGoodFunction', kind: 'expr', origin: 'userDefined' },
            ],
            diagnostics: [],
          },
        },
      });
    });

    expect(await screen.findByText('$init failed')).toBeInTheDocument();
    expect(
      port.sent.filter((message) => message.type === 'retryProjectRuntime'),
    ).toHaveLength(0);

    fireEvent.click(screen.getByRole('button', { name: 'Retry build' }));

    expect(await screen.findByText('Preparing current build…')).toBeInTheDocument();
    expect(
      port.sent.filter(
        (message) =>
          message.type === 'retryProjectRuntime' &&
          message.project === '/project' &&
          message.incarnation === 1,
      ),
    ).toHaveLength(1);
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
