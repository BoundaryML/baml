import { useState } from 'react';
import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { FunctionSidebar } from '../../pkg-playground/src/FunctionSidebar';

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
