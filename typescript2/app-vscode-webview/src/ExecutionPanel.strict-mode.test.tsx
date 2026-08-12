/** biome-ignore-all lint/style/useFilenamingConvention: preserve the established test filename */
import {
  type ControlFlowGraph,
  ExecutionPanel,
  type ProjectUpdate,
  type Run,
  type RuntimePort,
  type WorkerInMessage,
  type WorkerOutMessage,
} from '@b/pkg-playground';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { act, StrictMode } from 'react';
import { beforeAll, describe, expect, it, vi } from 'vitest';

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
        notification: {
          projects: ['project'],
          type: 'listProjects',
        },
        type: 'playgroundNotification',
      });
      port.emit({
        notification: {
          project: 'project',
          type: 'updateProject',
          update: {
            diagnostics: [],
            functions: [
              { kind: 'expr', name: 'ReadNote', origin: 'userDefined' },
            ],
            generation: 1,
            isBexCurrent: true,
          },
        },
        type: 'playgroundNotification',
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
        requestId: startRun!.requestId,
        run,
        type: 'runStarted',
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
        boundaryId: run.boundaryId,
        requestId: snapshot!.requestId,
        snapshot: { ...run, cursor: 1, status: 'running' },
        type: 'runSnapshot',
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
        notification: {
          projects: ['project'],
          type: 'listProjects',
        },
        type: 'playgroundNotification',
      });
      port.emit({
        notification: {
          project: 'project',
          type: 'updateProject',
          update: {
            diagnostics: [],
            functions: [
              { kind: 'expr', name: 'paulo.Hi', origin: 'userDefined' },
              {
                kind: 'expr',
                name: 'paulo.childFunc1',
                origin: 'userDefined',
              },
            ],
            isBexCurrent: true,
          },
        },
        type: 'playgroundNotification',
      });
    });

    await waitFor(() => {
      expect(
        port.sent.filter((msg) => msg.type === 'requestControlFlowGraph'),
      ).toHaveLength(2);
    });

    act(() => {
      port.emit({
        notification: {
          functionName: 'paulo.Hi',
          graph: graphFixture('paulo.Hi', ['childFunc1']),
          type: 'controlFlowGraphResult',
        },
        type: 'playgroundNotification',
      });
      port.emit({
        notification: {
          functionName: 'paulo.childFunc1',
          graph: graphFixture('paulo.childFunc1'),
          type: 'controlFlowGraphResult',
        },
        type: 'playgroundNotification',
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
            msg.type === 'startRun' && msg.functionName === 'paulo.childFunc1',
        ),
      ).toBe(true);
    });
  });

  it('ignores stale CFG responses after a project update', async () => {
    const port = new FakeRuntimePort();
    const functions = [
      { kind: 'expr' as const, name: 'Alpha', origin: 'userDefined' as const },
      { kind: 'expr' as const, name: 'Zulu', origin: 'userDefined' as const },
    ];

    render(<ExecutionPanel port={port} />);

    act(() => {
      port.emit({
        notification: { projects: ['project'], type: 'listProjects' },
        type: 'playgroundNotification',
      });
      port.emit({
        notification: {
          project: 'project',
          type: 'updateProject',
          update: {
            diagnostics: [],
            functions,
            generation: 1,
            isBexCurrent: true,
          },
        },
        type: 'playgroundNotification',
      });
    });

    const cfgRequests = () =>
      port.sent.filter(
        (
          message,
        ): message is Extract<
          WorkerInMessage,
          { type: 'requestControlFlowGraph' }
        > => message.type === 'requestControlFlowGraph',
      );
    await waitFor(() => expect(cfgRequests()).toHaveLength(2));
    const staleRequests = new Map(
      cfgRequests().map((request) => [request.functionName, request.requestId]),
    );

    act(() => {
      port.emit({
        notification: {
          project: 'project',
          type: 'updateProject',
          update: {
            diagnostics: [],
            functions,
            generation: 2,
            isBexCurrent: true,
          },
        },
        type: 'playgroundNotification',
      });
    });

    await waitFor(() => expect(cfgRequests()).toHaveLength(4));
    const currentRequests = new Map(
      cfgRequests()
        .slice(2)
        .map((request) => [request.functionName, request.requestId]),
    );

    act(() => {
      port.emit({
        functionName: 'Alpha',
        graph: graphFixtureWithNodeCount('Alpha', 1),
        requestId: cfgRequestId(staleRequests, 'Alpha'),
        type: 'controlFlowGraphResult',
      });
      port.emit({
        functionName: 'Zulu',
        graph: graphFixtureWithNodeCount('Zulu', 5),
        requestId: cfgRequestId(staleRequests, 'Zulu'),
        type: 'controlFlowGraphResult',
      });
    });

    fireEvent.click(
      await screen.findByRole('button', { name: 'Functions (2)' }),
    );
    const alpha = screen.getByRole('button', { name: 'Alpha' });
    const zulu = screen.getByRole('button', { name: 'Zulu' });
    expect(
      alpha.compareDocumentPosition(zulu) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).not.toBe(0);

    act(() => {
      port.emit({
        functionName: 'Alpha',
        graph: graphFixtureWithNodeCount('Alpha', 1),
        requestId: cfgRequestId(currentRequests, 'Alpha'),
        type: 'controlFlowGraphResult',
      });
      port.emit({
        functionName: 'Zulu',
        graph: graphFixtureWithNodeCount('Zulu', 5),
        requestId: cfgRequestId(currentRequests, 'Zulu'),
        type: 'controlFlowGraphResult',
      });
    });

    await waitFor(() => {
      expect(
        zulu.compareDocumentPosition(alpha) & Node.DOCUMENT_POSITION_FOLLOWING,
      ).not.toBe(0);
    });
  });
});

describe('ExecutionPanel run history', () => {
  it('opens the selected historical run in the graph tab', async () => {
    const port = new FakeRuntimePort();

    render(<ExecutionPanel port={port} />);

    act(() => {
      port.emit({
        notification: { projects: ['project'], type: 'listProjects' },
        type: 'playgroundNotification',
      });
      port.emit({
        notification: {
          project: 'project',
          type: 'updateProject',
          update: {
            diagnostics: [],
            functions: [
              { kind: 'expr', name: 'ReadNote', origin: 'userDefined' },
            ],
            generation: 1,
            isBexCurrent: true,
          },
        },
        type: 'playgroundNotification',
      });
    });

    fireEvent.click(
      await screen.findByRole('button', { name: 'Functions (1)' }),
    );
    fireEvent.click(await screen.findByRole('button', { name: 'ReadNote' }));
    fireEvent.click(await screen.findByRole('button', { name: 'Run' }));

    const startRun = await waitFor(() => {
      const message = port.sent.find(
        (
          candidate,
        ): candidate is Extract<WorkerInMessage, { type: 'startRun' }> =>
          candidate.type === 'startRun',
      );
      expect(message).toBeDefined();
      return message!;
    });
    const run = runFixture(startRun.project, startRun.functionName);
    act(() => {
      port.emit({
        requestId: startRun.requestId,
        run,
        type: 'runStarted',
      });
    });
    const snapshot = await waitFor(() => {
      const message = port.sent.find(
        (
          candidate,
        ): candidate is Extract<WorkerInMessage, { type: 'snapshot' }> =>
          candidate.type === 'snapshot',
      );
      expect(message).toBeDefined();
      return message!;
    });
    act(() => {
      port.emit({
        boundaryId: run.boundaryId,
        requestId: snapshot.requestId,
        snapshot: { ...run, cursor: 1, status: 'succeeded' },
        type: 'runSnapshot',
      });
    });

    await waitFor(() => {
      expect(
        screen.getByRole('button', {
          name: 'View ReadNote run in graph',
        }),
      ).toBeInTheDocument();
    });
    act(() => {
      port.emit({
        notification: {
          project: 'project',
          type: 'updateProject',
          update: {
            diagnostics: [],
            functions: [
              { kind: 'expr', name: 'ReadNote', origin: 'userDefined' },
            ],
            generation: 2,
            isBexCurrent: true,
          },
        },
        type: 'playgroundNotification',
      });
    });
    expect(
      screen.getByRole('button', { name: 'View ReadNote run in graph' }),
    ).toBeDisabled();

    act(() => {
      port.emit({
        notification: {
          project: 'project',
          type: 'updateProject',
          update: {
            diagnostics: [],
            functions: [
              { kind: 'expr', name: 'ReadNote', origin: 'userDefined' },
            ],
            generation: 1,
            isBexCurrent: true,
          },
        },
        type: 'playgroundNotification',
      });
    });
    fireEvent.click(
      screen.getByRole('button', { name: 'View ReadNote run in graph' }),
    );

    expect(screen.getByRole('tab', { name: 'Graph' })).toHaveAttribute(
      'data-state',
      'active',
    );
    expect(screen.getByText('Loading graph...')).toBeInTheDocument();
  });
});

describe('ExecutionPanel test previews', () => {
  it('hydrates legacy test args without running and releases selection on navigation', async () => {
    const port = new FakeRuntimePort();
    const projectUpdate: ProjectUpdate = {
      diagnostics: [],
      functions: [
        {
          capabilities: {
            buildRequest: true,
            clientName: 'Gpt5',
            renderPrompt: true,
          },
          kind: 'llm',
          name: 'ClassifySentiment',
          origin: 'userDefined',
          params: [
            {
              hasDefault: false,
              name: 'text',
              schema: { type: 'string' },
            },
          ],
        },
        { kind: 'expr', name: 'OtherFunction', origin: 'userDefined' },
      ],
      isBexCurrent: true,
      tests: [
        {
          argsJson: '{"text":"I absolutely love this feature"}',
          functionName: 'ClassifySentiment',
          name: 'HappySentiment',
        },
      ],
    };

    render(<ExecutionPanel port={port} />);

    act(() => {
      port.emit({
        notification: { projects: ['project'], type: 'listProjects' },
        type: 'playgroundNotification',
      });
      port.emit({
        notification: {
          project: 'project',
          type: 'updateProject',
          update: projectUpdate,
        },
        type: 'playgroundNotification',
      });
    });

    fireEvent.click(
      await screen.findByTitle('Use HappySentiment args for ClassifySentiment'),
    );
    fireEvent.click(await screen.findByRole('button', { name: 'raw' }));

    const rawInput = await screen.findByPlaceholderText('{"key": "value"}');
    expect(JSON.parse((rawInput as HTMLInputElement).value)).toEqual({
      text: 'I absolutely love this feature',
    });
    expect(screen.getByText('ClassifySentiment()')).toBeInTheDocument();
    expect(port.sent.some((message) => message.type === 'startRun')).toBe(
      false,
    );
    expect(port.sent.some((message) => message.type === 'startTestRun')).toBe(
      false,
    );

    act(() => {
      port.emit({
        context: {
          functionName: 'OtherFunction',
          isWorkflow: false,
          sourceExprId: null,
          testName: null,
          workflowMemberships: [],
        },
        type: 'cursorContext',
      });
    });
    expect(await screen.findByText('OtherFunction()')).toBeInTheDocument();

    act(() => {
      port.emit({
        notification: {
          project: 'project',
          type: 'updateProject',
          update: {
            ...projectUpdate,
            tests: projectUpdate.tests?.map((test) => ({
              ...test,
              argsJson: '{"text":"source edit"}',
            })),
          },
        },
        type: 'playgroundNotification',
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
        notification: { projects: ['project'], type: 'listProjects' },
        type: 'playgroundNotification',
      });
      port.emit({
        notification: {
          project: 'project',
          type: 'updateProject',
          update: {
            diagnostics: [],
            functions: [
              {
                kind: 'expr',
                name: 'Greet',
                origin: 'userDefined',
                params: [
                  {
                    hasDefault: false,
                    name: 'name',
                    schema: { type: 'string' },
                  },
                  {
                    hasDefault: false,
                    name: 'color',
                    schema: { name: 'user.Color', type: 'ref' },
                  },
                ],
              },
            ],
            isBexCurrent: true,
            types: {
              'user.Color': {
                kind: 'enum',
                values: ['Red', 'Green', 'Blue'],
              },
            },
          },
        },
        type: 'playgroundNotification',
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
      color: { $baml: { enum: 'user.Color', value: 'Green' } },
      name: 'Ada',
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
        notification: { projects: ['project'], type: 'listProjects' },
        type: 'playgroundNotification',
      });
      port.emit({
        notification: {
          project: 'project',
          type: 'updateProject',
          update: {
            diagnostics: [],
            functions: [
              {
                kind: 'expr',
                name: 'Greet',
                origin: 'userDefined',
                params: [
                  {
                    hasDefault: false,
                    name: 'color',
                    schema: { name: 'user.Color', type: 'ref' },
                  },
                ],
              },
              { kind: 'expr', name: 'Zero', origin: 'userDefined', params: [] },
            ],
            isBexCurrent: true,
            types: {
              'user.Color': {
                kind: 'enum',
                values: ['Red', 'Green', 'Blue'],
              },
            },
          },
        },
        type: 'playgroundNotification',
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
        notification: { projects: ['project'], type: 'listProjects' },
        type: 'playgroundNotification',
      });
      port.emit({
        notification: {
          project: 'project',
          type: 'updateProject',
          update: {
            diagnostics: [],
            functions: [
              {
                kind: 'expr',
                name: 'EchoPerson',
                origin: 'userDefined',
                params: [
                  {
                    hasDefault: false,
                    name: 'person',
                    schema: { name: 'user.Person', type: 'ref' },
                  },
                ],
              },
            ],
            isBexCurrent: true,
            types: {
              'user.Person': {
                fields: [
                  { name: 'name', schema: { type: 'string' } },
                  { name: 'active', schema: { type: 'bool' } },
                ],
                kind: 'class',
              },
            },
          },
        },
        type: 'playgroundNotification',
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
        notification: {
          project: 'project',
          type: 'updateProject',
          update: {
            diagnostics: [],
            functions: [
              {
                kind: 'expr',
                name: 'EchoPerson',
                origin: 'userDefined',
                params: [
                  {
                    hasDefault: false,
                    name: 'person',
                    schema: { name: 'user.Person', type: 'ref' },
                  },
                ],
              },
            ],
            isBexCurrent: true,
            types: {
              'user.Person': {
                fields: [
                  { name: 'name', schema: { type: 'string' } },
                  { name: 'active', schema: { type: 'bool' } },
                  { name: 'age', schema: { type: 'int' } },
                ],
                kind: 'class',
              },
            },
          },
        },
        type: 'playgroundNotification',
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
          active: false,
          age: 0,
          name: 'Ada Lovelace',
        },
      });
    });
  });

  it('serializes untouched defaults inside a required nested class', async () => {
    const port = new FakeRuntimePort();

    render(<ExecutionPanel port={port} />);

    act(() => {
      port.emit({
        notification: { projects: ['project'], type: 'listProjects' },
        type: 'playgroundNotification',
      });
      port.emit({
        notification: {
          project: 'project',
          type: 'updateProject',
          update: {
            diagnostics: [],
            functions: [
              {
                kind: 'expr',
                name: 'EchoEnvelope',
                origin: 'userDefined',
                params: [
                  {
                    hasDefault: false,
                    name: 'envelope',
                    schema: { name: 'user.Envelope', type: 'ref' },
                  },
                ],
              },
            ],
            isBexCurrent: true,
            types: {
              'user.Envelope': {
                fields: [
                  {
                    name: 'flag',
                    schema: { name: 'user.Flag', type: 'ref' },
                  },
                ],
                kind: 'class',
              },
              'user.Flag': {
                fields: [{ name: 'active', schema: { type: 'bool' } }],
                kind: 'class',
              },
            },
          },
        },
        type: 'playgroundNotification',
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
        notification: { projects: ['project'], type: 'listProjects' },
        type: 'playgroundNotification',
      });
      port.emit({
        notification: {
          project: 'project',
          type: 'updateProject',
          update: {
            diagnostics: [],
            functions: [
              {
                kind: 'expr',
                name: 'Mix',
                origin: 'userDefined',
                params: [
                  {
                    hasDefault: false,
                    name: 'x',
                    schema: {
                      type: 'union',
                      variants: [{ type: 'int' }, { type: 'float' }],
                    },
                  },
                ],
              },
            ],
            isBexCurrent: true,
            types: {},
          },
        },
        type: 'playgroundNotification',
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
        notification: { projects: ['project'], type: 'listProjects' },
        type: 'playgroundNotification',
      });
      port.emit({
        notification: {
          project: 'project',
          type: 'updateProject',
          update: {
            diagnostics: [],
            functions: [
              {
                kind: 'expr',
                name: 'Walk',
                origin: 'userDefined',
                params: [
                  {
                    hasDefault: false,
                    name: 't',
                    schema: { name: 'user.Tree', type: 'ref' },
                  },
                ],
              },
            ],
            isBexCurrent: true,
            types: {
              'user.Tree': {
                fields: [
                  { name: 'value', schema: { type: 'int' } },
                  {
                    name: 'children',
                    schema: {
                      item: { name: 'user.Tree', type: 'ref' },
                      type: 'list',
                    },
                  },
                ],
                kind: 'class',
              },
            },
          },
        },
        type: 'playgroundNotification',
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
        children: [{ $baml: { type: 'user.Tree' }, children: [], value: 7 }],
        value: 0,
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
        notification: { projects: ['project'], type: 'listProjects' },
        type: 'playgroundNotification',
      });
      port.emit({
        notification: {
          project: 'project',
          type: 'updateProject',
          update: {
            diagnostics: [],
            functions: [
              {
                kind: 'expr',
                name: 'Loop',
                origin: 'userDefined',
                params: [
                  {
                    hasDefault: false,
                    name: 'a',
                    schema: { name: 'user.A', type: 'ref' },
                  },
                ],
              },
            ],
            isBexCurrent: true,
            types: {
              'user.A': {
                kind: 'alias',
                schema: {
                  type: 'union',
                  variants: [{ name: 'user.A', type: 'ref' }, { type: 'int' }],
                },
              },
            },
          },
        },
        type: 'playgroundNotification',
      });
    });

    fireEvent.click(
      await screen.findByRole('button', { name: 'Functions (1)' }),
    );
    fireEvent.click(await screen.findByRole('button', { name: 'Loop' }));

    // Union chips render; the self-referential variant is reachable but its
    // widget degrades to the raw-JSON textarea.
    fireEvent.click(await screen.findByRole('button', { name: 'A' }));
    expect(await screen.findByPlaceholderText('JSON (A)')).toBeInTheDocument();
  });

  it('normalizes a bare-enum host seed into the wire marker', async () => {
    const port = new FakeRuntimePort();

    render(<ExecutionPanel initialArgsJson='{"c":"Red"}' port={port} />);

    act(() => {
      port.emit({
        notification: { projects: ['project'], type: 'listProjects' },
        type: 'playgroundNotification',
      });
      port.emit({
        notification: {
          project: 'project',
          type: 'updateProject',
          update: {
            diagnostics: [],
            functions: [
              {
                kind: 'expr',
                name: 'IsRed',
                origin: 'userDefined',
                params: [
                  {
                    hasDefault: false,
                    name: 'c',
                    schema: { name: 'user.Color', type: 'ref' },
                  },
                ],
              },
            ],
            isBexCurrent: true,
            types: {
              'user.Color': {
                kind: 'enum',
                values: ['Red', 'Green', 'Blue'],
              },
            },
          },
        },
        type: 'playgroundNotification',
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
        notification: { projects: ['project'], type: 'listProjects' },
        type: 'playgroundNotification',
      });
      port.emit({
        notification: {
          project: 'project',
          type: 'updateProject',
          update: {
            diagnostics: [],
            functions: [
              { kind: 'expr', name: 'Zero', origin: 'userDefined', params: [] },
            ],
            isBexCurrent: true,
          },
        },
        type: 'playgroundNotification',
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
    calls: [],
    cancellation: null,
    completedAtMs: null,
    createdAtMs: 100,
    cursor: 0,
    diagnostics: [],
    error: null,
    graphRuntimeOverlay: null,
    payloads: [],
    request: {
      argsSummary: '{}',
      optionsSummary: null,
      projectGeneration: 1,
      projectId,
      target: { functionName, kind: 'function' },
    },
    result: null,
    rootCallNodeId: null,
    startedAtMs: null,
    status: 'pending',
    target: { functionName, kind: 'function' },
    threads: [],
    timeAnchor: {
      epochCreatedAtMs: 100,
      traceZeroNs: '0',
    },
    visibility: { kind: 'history' },
  };
}

function graphFixture(
  functionName: string,
  calleeNames: string[] = [],
): ControlFlowGraph {
  return {
    edgesBySrc: {},
    nodes: {
      '1': {
        calleeNames,
        id: 1,
        isContainer: true,
        label: functionName,
        logFilterKey: functionName,
        nodeType: 'functionRoot',
        parentNodeId: null,
        sourceExpr: null,
      },
    },
  };
}

function graphFixtureWithNodeCount(
  functionName: string,
  nodeCount: number,
): ControlFlowGraph {
  const graph = graphFixture(functionName);
  for (let id = 2; id <= nodeCount; id += 1) {
    graph.nodes[String(id)] = {
      id,
      isContainer: false,
      label: `node ${id}`,
      logFilterKey: `${functionName}:${id}`,
      nodeType: 'return',
      parentNodeId: 1,
      sourceExpr: null,
    };
  }
  return graph;
}

function cfgRequestId(
  requestIds: ReadonlyMap<string, number | undefined>,
  functionName: string,
): number {
  const requestId = requestIds.get(functionName);
  if (requestId === undefined) {
    throw new Error(`missing CFG request ID for ${functionName}`);
  }
  return requestId;
}
