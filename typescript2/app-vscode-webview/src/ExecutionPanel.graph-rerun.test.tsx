import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from '@testing-library/react';
import { encodeRunArgs } from '@b/pkg-proto';
import { beforeAll, describe, expect, it, vi } from 'vitest';
import {
  ExecutionPanel,
  type ProjectUpdate,
  type RuntimePort,
  type WorkerInMessage,
  type WorkerOutMessage,
} from '@b/pkg-playground';

beforeAll(() => {
  HTMLElement.prototype.scrollTo ??= vi.fn();
});

function announceProject(port: FakeRuntimePort): void {
  const update: ProjectUpdate = {
    isBexCurrent: true,
    functions: [{ name: 'PlanTrip', kind: 'expr', origin: 'userDefined' }],
    diagnostics: [],
  };
  act(() => {
    port.emit({
      type: 'playgroundNotification',
      notification: { type: 'listProjects', projects: ['project'] },
    });
    port.emit({
      type: 'playgroundNotification',
      notification: { type: 'updateProject', project: 'project', update },
    });
  });
}

async function selectPlanTrip(): Promise<void> {
  fireEvent.click(await screen.findByRole('button', { name: 'Functions (1)' }));
  fireEvent.click(await screen.findByRole('button', { name: 'PlanTrip' }));
}

describe('ExecutionPanel graph reruns', () => {
  it('edits arguments and starts a rerun without leaving the graph', async () => {
    const port = new FakeRuntimePort();
    render(
      <ExecutionPanel
        port={port}
        initialTab="graph"
        initialArgsJson='{"destination":"Paris"}'
      />,
    );

    announceProject(port);
    await selectPlanTrip();

    const graphArgsEditor = await screen.findByTestId('graph-args-editor');
    const argsInput = within(graphArgsEditor).getByRole('textbox', {
      name: 'Arguments JSON',
    });
    expect(argsInput).toHaveValue('{"destination":"Paris"}');

    fireEvent.change(argsInput, {
      target: { value: '{"destination":"Kyoto"}' },
    });
    fireEvent.click(
      within(graphArgsEditor).getByRole('button', { name: 'Run' }),
    );

    await waitFor(() => {
      const startRun = port.sent.find(
        (message): message is Extract<WorkerInMessage, { type: 'startRun' }> =>
          message.type === 'startRun',
      );
      expect(startRun?.functionName).toBe('PlanTrip');
      expect(startRun?.argsBytes).toEqual(
        encodeRunArgs({ destination: 'Kyoto' }),
      );
    });
    expect(screen.getByRole('tab', { name: 'Graph' })).toHaveAttribute(
      'data-state',
      'active',
    );
  });

  it('shows argument validation errors in the graph and does not start a run', async () => {
    const port = new FakeRuntimePort();
    render(
      <ExecutionPanel port={port} initialTab="graph" initialArgsJson="{}" />,
    );

    announceProject(port);
    await selectPlanTrip();

    const graphArgsEditor = await screen.findByTestId('graph-args-editor');
    fireEvent.change(
      within(graphArgsEditor).getByRole('textbox', { name: 'Arguments JSON' }),
      { target: { value: '{"destination":' } },
    );
    fireEvent.click(
      within(graphArgsEditor).getByRole('button', { name: 'Run' }),
    );

    expect(
      await screen.findByText(/Unexpected end of JSON input/),
    ).toBeVisible();
    expect(port.sent.some((message) => message.type === 'startRun')).toBe(
      false,
    );
  });
});

class FakeRuntimePort implements RuntimePort {
  sent: WorkerInMessage[] = [];
  private handlers = new Set<(message: WorkerOutMessage) => void>();

  postMessage(message: WorkerInMessage): void {
    this.sent.push(message);
  }

  onMessage(handler: (message: WorkerOutMessage) => void): () => void {
    this.handlers.add(handler);
    return () => {
      this.handlers.delete(handler);
    };
  }

  emit(message: WorkerOutMessage): void {
    for (const handler of this.handlers) handler(message);
  }

  dispose(): void {
    this.handlers.clear();
  }
}
