import { StrictMode, act } from 'react';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeAll, describe, expect, it, vi } from 'vitest';
import {
  ExecutionPanel,
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
        runId: run.runId,
        snapshot: { ...run, status: 'running', cursor: 1 },
      });
    });

    expect(await screen.findByText('running...')).toBeInTheDocument();
    expect(
      screen.queryByText('Press Run to execute ReadNote()'),
    ).not.toBeInTheDocument();
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
    runId: 'baml_run_1_00000000000000000000000000000001',
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
