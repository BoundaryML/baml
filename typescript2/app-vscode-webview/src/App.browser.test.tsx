import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import App from './App';

describe('App (Browser)', () => {
  describe('initial render', () => {
    it('renders loading state or ExecutionPanel', async () => {
      render(<App />);

      await screen.findByText(/Select a function to run|Connecting to playground server/);
    });

    it('shows the sidebar sections and empty editor state', async () => {
      render(<App />);

      expect(await screen.findByText('Functions')).toBeInTheDocument();
      expect(screen.getByText('Tests')).toBeInTheDocument();
      expect(screen.getByText('Select a function to run')).toBeInTheDocument();
    });

  });
});
