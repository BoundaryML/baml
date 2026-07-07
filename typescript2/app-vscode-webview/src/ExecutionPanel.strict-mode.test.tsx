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

describe('ExecutionPanel StrictMode lifecycle', () => {
  beforeAll(() => {
    HTMLElement.prototype.scrollTo ??= vi.fn();
  });

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
                    schema: {
                      type: 'enum',
                      name: 'user.Color',
                      values: ['Red', 'Green', 'Blue'],
                    },
                  },
                ],
              },
            ],
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
