/** biome-ignore-all lint/style/useFilenamingConvention: a test file is named for the component it tests */

import {
  ExecutionPanel,
  type ProjectUpdate,
  type RuntimePort,
  type WorkerInMessage,
  type WorkerOutMessage,
} from '@b/pkg-playground';
import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from '@testing-library/react';
import { beforeAll, describe, expect, it, vi } from 'vitest';

beforeAll(() => {
  HTMLElement.prototype.scrollTo ??= vi.fn();
});

const PREPARING_TEXT = 'Preparing current build…';

function projectUpdate(overrides: Partial<ProjectUpdate> = {}): ProjectUpdate {
  return {
    diagnostics: [],
    functions: [{ kind: 'expr', name: 'ReadNote', origin: 'userDefined' }],
    isBexCurrent: true,
    ...overrides,
  };
}

function announceProject(port: FakeRuntimePort, update: ProjectUpdate): void {
  act(() => {
    port.emit({
      notification: { projects: [{ path: 'project' }], type: 'listProjects' },
      type: 'playgroundNotification',
    });
    port.emit({
      notification: { project: 'project', type: 'updateProject', update },
      type: 'playgroundNotification',
    });
  });
}

async function selectReadNote(): Promise<void> {
  fireEvent.click(await screen.findByRole('button', { name: 'Functions (1)' }));
  fireEvent.click(await screen.findByRole('button', { name: 'ReadNote' }));
}

describe('ExecutionPanel run gating (fail-closed server)', () => {
  it('disables Run and shows the preparing state while the build is stale, keeping the catalog visible', async () => {
    const port = new FakeRuntimePort();
    render(<ExecutionPanel port={port} />);

    announceProject(port, projectUpdate({ isBexCurrent: false }));
    await selectReadNote();

    // The previous function listing stays visible…
    expect(
      screen.getByRole('button', { name: 'ReadNote' }),
    ).toBeInTheDocument();
    // …but runtime-derived controls are gated behind the preparing state.
    expect(await screen.findByText(PREPARING_TEXT)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Run' })).toBeDisabled();

    fireEvent.click(screen.getByRole('button', { name: 'Run' }));
    expect(port.sent.some((msg) => msg.type === 'startRun')).toBe(false);

    // The next current update re-enables Run automatically.
    announceProject(port, projectUpdate({ isBexCurrent: true }));
    await waitFor(() => {
      expect(screen.getByRole('button', { name: 'Run' })).toBeEnabled();
    });
    expect(screen.queryByText(PREPARING_TEXT)).not.toBeInTheDocument();
  });

  it('renders a projectNotReady run rejection as the transient preparing state, not a raw error', async () => {
    const port = new FakeRuntimePort();
    render(<ExecutionPanel port={port} />);

    announceProject(port, projectUpdate({ isBexCurrent: true }));
    await selectReadNote();
    fireEvent.click(await screen.findByRole('button', { name: 'Run' }));

    const startRun = await waitFor(() => {
      const msg = port.sent.find(
        (
          candidate,
        ): candidate is Extract<WorkerInMessage, { type: 'startRun' }> =>
          candidate.type === 'startRun',
      );
      expect(msg).toBeDefined();
      return msg!;
    });

    // The fail-closed server refuses the run while a rebuild is pending.
    act(() => {
      port.emit({
        code: 'projectNotReady',
        message: 'Cannot start run: rebuild pending',
        requestId: startRun.requestId,
        type: 'commandError',
      });
    });

    expect(await screen.findByText(PREPARING_TEXT)).toBeInTheDocument();
    // No raw error text or code leaks into the panel.
    expect(screen.queryByText(/projectNotReady/)).not.toBeInTheDocument();
    expect(screen.queryByText(/rebuild pending/)).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Run' })).toBeDisabled();

    // The next ProjectUpdate with a current build clears the state…
    announceProject(port, projectUpdate({ isBexCurrent: true }));
    await waitFor(() => {
      expect(screen.queryByText(PREPARING_TEXT)).not.toBeInTheDocument();
    });

    // …and Run works again.
    const sentBefore = port.sent.filter(
      (msg) => msg.type === 'startRun',
    ).length;
    fireEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => {
      expect(
        port.sent.filter((msg) => msg.type === 'startRun').length,
      ).toBeGreaterThan(sentBefore);
    });
  });

  it('shows the diagnostics banner (not the preparing spinner) for compile errors', async () => {
    const port = new FakeRuntimePort();
    render(<ExecutionPanel port={port} />);

    announceProject(
      port,
      projectUpdate({
        diagnostics: [{ message: 'missing }', severity: 'error' }],
        isBexCurrent: false,
      }),
    );
    await selectReadNote();

    expect(
      await screen.findByText(/1 error — current build unavailable/),
    ).toBeInTheDocument();
    expect(screen.queryByText(PREPARING_TEXT)).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Run' })).toBeDisabled();
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
