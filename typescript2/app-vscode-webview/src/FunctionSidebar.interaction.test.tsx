// biome-ignore-all lint/style/useFilenamingConvention: Preserve the existing test filename.

import { fireEvent, render, screen, within } from '@testing-library/react';
import { useState } from 'react';
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';

import { FunctionSidebar } from '../../pkg-playground/src/FunctionSidebar';

const collapsePendingMessage =
  'Will collapse when this folder is no longer kept open automatically';

beforeAll(() => {
  vi.stubGlobal(
    'ResizeObserver',
    class ResizeObserver {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  );
});

afterAll(() => {
  vi.unstubAllGlobals();
});

describe('FunctionSidebar folder disclosure', () => {
  it('shows a pending indicator before collapsing after navigation leaves the folder', () => {
    render(<SidebarHarness />);

    fireEvent.click(screen.getByRole('button', { name: 'Functions (2)' }));
    const folder = screen.getByRole('button', { name: 'demo (1)' });
    expect(folder).toHaveAttribute('aria-expanded', 'true');
    expect(folder.querySelector('.lucide-folder')).not.toBeInTheDocument();
    expect(folder.querySelector('.lucide-chevron-right')).toBeInTheDocument();

    fireEvent.click(folder);

    expect(folder).toHaveAttribute('aria-expanded', 'true');
    expect(folder).toHaveAttribute('data-collapse-pending', 'true');
    expect(screen.getByText(collapsePendingMessage)).toBeInTheDocument();
    expect(folder.querySelector('svg')).toHaveClass('shrink-0');
    expect(folder.querySelector('svg')).toHaveClass('text-vsc-accent');
    expect(folder.querySelector('svg')).not.toHaveClass('rotate-90');

    fireEvent.click(screen.getByRole('button', { name: 'Main' }));

    expect(folder).toHaveAttribute('aria-expanded', 'false');
    expect(folder).not.toHaveAttribute('data-collapse-pending');
  });

  it('cancels pending collapse when the forced-open folder is clicked again', () => {
    render(<SidebarHarness />);

    fireEvent.click(screen.getByRole('button', { name: 'Functions (2)' }));
    const folder = screen.getByRole('button', { name: 'demo (1)' });

    fireEvent.click(folder);
    expect(folder).toHaveAttribute('data-collapse-pending', 'true');

    fireEvent.click(folder);
    expect(folder).not.toHaveAttribute('data-collapse-pending');
    expect(folder.querySelector('svg')).toHaveClass('rotate-90');
    expect(folder.querySelector('svg')).not.toHaveClass('text-vsc-accent');

    fireEvent.click(screen.getByRole('button', { name: 'Main' }));
    expect(folder).toHaveAttribute('aria-expanded', 'true');
  });
});

describe('FunctionSidebar function details and sorting', () => {
  it('keeps internal-origin badges out of function accessible names', () => {
    render(
      <FunctionSidebar
        functions={[{ kind: 'expr', name: 'BuiltIn', origin: 'internal' }]}
        internalFunctionCount={1}
        onRefreshTests={vi.fn()}
        onSelectFn={vi.fn()}
        selectedFn={null}
        showInternalFunctions
        workflowNodeCounts={new Map([['BuiltIn', 12]])}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'Functions (1)' }));

    expect(screen.getByRole('button', { name: 'BuiltIn' })).toBeInTheDocument();
    expect(
      screen.queryByRole('button', { name: 'BuiltIn internal' }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole('button', { name: 'BuiltIn 12' }),
    ).not.toBeInTheDocument();
  });

  it('keeps older function metadata payloads usable', async () => {
    render(<SidebarHarness />);

    fireEvent.click(screen.getByRole('button', { name: 'Functions (2)' }));
    fireEvent.focus(screen.getByRole('button', { name: 'Main' }));

    expect(
      (await screen.findAllByText('Source position unavailable')).length,
    ).toBeGreaterThan(0);
    expect(
      screen.getAllByText('Call graph nodes: calculating…').length,
    ).toBeGreaterThan(0);
  });

  it('shows signature, source position, and node count when a row receives focus', async () => {
    render(<FunctionDetailsHarness />);

    fireEvent.click(screen.getByRole('button', { name: 'Functions (3)' }));
    fireEvent.focus(screen.getByRole('button', { name: 'Middle' }));

    expect(
      (
        await screen.findAllByText(
          'function Middle(input: string) -> string throws never',
        )
      ).length,
    ).toBeGreaterThan(0);
    expect(
      screen.getAllByText('baml_src/main.baml at 7:10').length,
    ).toBeGreaterThan(0);
    expect(screen.getAllByText('Call graph nodes: 10').length).toBeGreaterThan(
      0,
    );
  });

  it('shows the same details when a row is hovered with a mouse', async () => {
    render(<FunctionDetailsHarness />);

    fireEvent.click(screen.getByRole('button', { name: 'Functions (3)' }));
    const alphaRow = screen.getByRole('button', { name: 'Alpha' });
    expect(within(alphaRow).queryByText('1')).not.toBeInTheDocument();

    fireEvent.pointerMove(alphaRow, {
      pointerType: 'mouse',
    });

    expect(
      (await screen.findAllByText('function Alpha() -> null throws never'))
        .length,
    ).toBeGreaterThan(0);
    expect(
      screen.getAllByText('baml_src/main.baml at 2:10').length,
    ).toBeGreaterThan(0);
    expect(screen.getAllByText('Call graph nodes: 1').length).toBeGreaterThan(
      0,
    );
  });

  it('defaults to node count and switches to natural alphanumeric order', () => {
    render(<NaturalSortHarness />);

    fireEvent.click(screen.getByRole('button', { name: 'Functions (4)' }));
    expect(
      functionRows(['Function1', 'function2', 'Function2', 'Function10']),
    ).toEqual(['Function10', 'Function2', 'Function1', 'function2']);

    fireEvent.click(
      screen.getByRole('button', {
        name: 'Sort order: Call graph node count',
      }),
    );
    fireEvent.click(screen.getByRole('radio', { name: 'Alphanumeric' }));

    expect(
      screen.getByRole('button', { name: 'Sort order: Alphanumeric' }),
    ).toBeInTheDocument();
    expect(
      functionRows(['Function1', 'function2', 'Function2', 'Function10']),
    ).toEqual(['Function1', 'function2', 'Function2', 'Function10']);
  });

  it('does not render a one-node badge but keeps its tooltip and sort value', async () => {
    render(<FunctionDetailsHarness />);

    fireEvent.click(screen.getByRole('button', { name: 'Functions (3)' }));
    expect(functionRows()).toEqual(['Zulu', 'Middle', 'Alpha']);

    const alphaRow = screen.getByRole('button', { name: 'Alpha' });
    expect(within(alphaRow).queryByText('1')).not.toBeInTheDocument();
    fireEvent.focus(alphaRow);

    expect(
      (await screen.findAllByText('Call graph nodes: 1')).length,
    ).toBeGreaterThan(0);
  });

  it('keeps the selected sort order across mounted project updates', () => {
    const { rerender } = render(<FunctionDetailsHarness />);

    fireEvent.click(screen.getByRole('button', { name: 'Functions (3)' }));
    fireEvent.click(
      screen.getByRole('button', {
        name: 'Sort order: Call graph node count',
      }),
    );
    fireEvent.click(screen.getByRole('radio', { name: 'Alphanumeric' }));

    rerender(<FunctionDetailsHarness projectVersion={2} />);

    expect(
      screen.getByRole('button', { name: 'Sort order: Alphanumeric' }),
    ).toBeInTheDocument();
    expect(functionRows(['Alpha', 'Beta', 'Middle', 'Zulu'])).toEqual([
      'Alpha',
      'Beta',
      'Middle',
      'Zulu',
    ]);
  });
});

function SidebarHarness() {
  const [selectedFn, setSelectedFn] = useState<string | null>('demo.Foo');

  return (
    <FunctionSidebar
      functions={[
        { kind: 'expr', name: 'Main', origin: 'userDefined' },
        { kind: 'expr', name: 'demo.Foo', origin: 'userDefined' },
      ]}
      internalFunctionCount={0}
      onRefreshTests={vi.fn()}
      onSelectFn={setSelectedFn}
      selectedFn={selectedFn}
      showInternalFunctions={false}
    />
  );
}

function NaturalSortHarness() {
  const functions = [
    functionDetails('Function10'),
    functionDetails('function2'),
    functionDetails('Function2'),
    functionDetails('Function1'),
  ];

  return (
    <FunctionSidebar
      functions={functions}
      internalFunctionCount={0}
      onRefreshTests={vi.fn()}
      onSelectFn={vi.fn()}
      selectedFn={null}
      showInternalFunctions={false}
      workflowNodeCounts={
        new Map([
          ['Function10', 20],
          ['function2', 1],
          ['Function2', 10],
          ['Function1', 5],
        ])
      }
    />
  );
}

function functionDetails(name: string) {
  return {
    kind: 'expr' as const,
    name,
    origin: 'userDefined' as const,
    signature: `function ${name}() -> null throws never`,
    sourcePosition: { column: 10, file: 'baml_src/main.baml', line: 1 },
  };
}

function FunctionDetailsHarness({
  projectVersion = 1,
}: {
  projectVersion?: number;
}) {
  const functions = [
    {
      kind: 'expr' as const,
      name: 'Zulu',
      origin: 'userDefined' as const,
      signature: 'function Zulu() -> null throws never',
      sourcePosition: { column: 10, file: 'baml_src/main.baml', line: 12 },
    },
    {
      kind: 'expr' as const,
      name: 'Alpha',
      origin: 'userDefined' as const,
      signature: 'function Alpha() -> null throws never',
      sourcePosition: { column: 10, file: 'baml_src/main.baml', line: 2 },
    },
    {
      kind: 'expr' as const,
      name: 'Middle',
      origin: 'userDefined' as const,
      signature: 'function Middle(input: string) -> string throws never',
      sourcePosition: { column: 10, file: 'baml_src/main.baml', line: 7 },
    },
  ];
  const workflowNodeCounts = new Map([
    ['Zulu', 20],
    ['Alpha', 1],
    ['Middle', 10],
  ]);
  if (projectVersion === 2) {
    functions.push({
      kind: 'expr',
      name: 'Beta',
      origin: 'userDefined',
      signature: 'function Beta() -> null throws never',
      sourcePosition: { column: 10, file: 'baml_src/updated.baml', line: 3 },
    });
    workflowNodeCounts.set('Beta', 30);
  }

  return (
    <FunctionSidebar
      functions={functions}
      internalFunctionCount={0}
      onRefreshTests={vi.fn()}
      onSelectFn={vi.fn()}
      selectedFn={null}
      showInternalFunctions={false}
      workflowNodeCounts={workflowNodeCounts}
    />
  );
}

function functionRows(names = ['Alpha', 'Middle', 'Zulu']) {
  return names
    .map((name) => ({
      element: screen.getByRole('button', { name }),
      name,
    }))
    .sort((left, right) =>
      left.element.compareDocumentPosition(right.element) &
      Node.DOCUMENT_POSITION_FOLLOWING
        ? -1
        : 1,
    )
    .map(({ name }) => name);
}
