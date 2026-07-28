import { useState } from 'react';
import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { FunctionSidebar } from '../../pkg-playground/src/FunctionSidebar';
import {
  SIDEBAR_LEAF_ICON_CLASS,
  SIDEBAR_LEAF_ROW_CLASS,
} from '../../pkg-playground/src/function-sidebar-row-styles';

const collapsePendingMessage =
  'Will collapse when this folder is no longer kept open automatically';

describe('FunctionSidebar folder disclosure', () => {
  it('shows a pending indicator before collapsing after navigation leaves the folder', () => {
    render(<SidebarHarness />);

    fireEvent.click(screen.getByRole('button', { name: 'Functions (2)' }));
    const folder = screen.getByRole('button', { name: 'demo (1)' });
    expect(folder).toHaveAttribute('aria-expanded', 'true');

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

describe('FunctionSidebar leaf rows', () => {
  it('uses the same geometry for function and test rows without losing test controls or results', () => {
    const onRunTest = vi.fn();
    render(
      <FunctionSidebar
        functions={[
          { name: 'Main', kind: 'expr', origin: 'userDefined' },
        ]}
        showInternalFunctions={false}
        internalFunctionCount={0}
        selectedFn={null}
        onSelectFn={vi.fn()}
        onRefreshTests={vi.fn()}
        testTree={[{ type: 'test', name: 'SimpleTest' }]}
        onRunTest={onRunTest}
        testRunResults={new Map([['SimpleTest', { outcome: 'pass' }]])}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'Functions (1)' }));
    const functionRow = screen.getByRole('button', { name: 'Main' });
    const testRow = screen.getByText('SimpleTest').closest('div');
    expect(testRow).not.toBeNull();

    for (const className of SIDEBAR_LEAF_ROW_CLASS.split(' ')) {
      expect(functionRow).toHaveClass(className);
      expect(testRow).toHaveClass(className);
    }
    expect(functionRow).toHaveStyle({ paddingLeft: '20px' });
    expect(testRow).toHaveStyle({ paddingLeft: '20px' });

    for (const className of SIDEBAR_LEAF_ICON_CLASS.split(' ')) {
      expect(functionRow.querySelector('svg')).toHaveClass(className);
      expect(testRow?.querySelector('svg')).toHaveClass(className);
    }

    expect(testRow).toHaveTextContent('pass');
    fireEvent.click(screen.getByTitle('Run test: SimpleTest'));
    expect(onRunTest).toHaveBeenCalledWith('SimpleTest');
  });
});

function SidebarHarness() {
  const [selectedFn, setSelectedFn] = useState<string | null>('demo.Foo');

  return (
    <FunctionSidebar
      functions={[
        { name: 'Main', kind: 'expr', origin: 'userDefined' },
        { name: 'demo.Foo', kind: 'expr', origin: 'userDefined' },
      ]}
      showInternalFunctions={false}
      internalFunctionCount={0}
      selectedFn={selectedFn}
      onSelectFn={setSelectedFn}
      onRefreshTests={vi.fn()}
    />
  );
}
