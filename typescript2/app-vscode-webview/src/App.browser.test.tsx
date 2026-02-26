import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import App from './App';

describe('App (Browser)', () => {
  describe('initial render', () => {
    it('renders loading state or ExecutionPanel', async () => {
      render(<App />);

      await screen.findByText(/Select a function to run|Connecting to playground server/);
    });

    it('shows ENV section when ExecutionPanel is rendered', async () => {
      render(<App />);

      await screen.findByText('ENV');
    });

    it('shows functions area (empty or list)', async () => {
      render(<App />);

      expect(await screen.findByText('No functions yet')).toBeInTheDocument();
      expect(screen.getByText('Select a function to run')).toBeInTheDocument();
    });

    it('renders a key/value input row in ENV bar', async () => {
      render(<App />);

      await screen.findByPlaceholderText('KEY');
      expect(screen.getByPlaceholderText('value')).toBeInTheDocument();
    });
  });
});
