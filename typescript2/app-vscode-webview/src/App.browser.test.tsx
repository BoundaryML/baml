import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import App from './App';

describe('App (Browser)', () => {
  describe('initial render', () => {
    it('should render the main app container', () => {
      render(<App />);

      const main = screen.getByRole('main');
      expect(main).toBeInTheDocument();
      expect(main).toHaveClass('app');
    });

    it('should render the header with title', () => {
      render(<App />);

      const heading = screen.getByRole('heading', { level: 1 });
      expect(heading).toBeInTheDocument();
      expect(heading).toHaveTextContent('Standalone Playground');
    });

    it('should render the description paragraph', () => {
      render(<App />);

      const description = screen.getByText(
        /shared ui components and state management come from the common package/i
      );
      expect(description).toBeInTheDocument();
    });

    it('should render the app header section with correct class', () => {
      render(<App />);

      // Use querySelector since there are multiple headers
      const main = screen.getByRole('main');
      const appHeader = main.querySelector('.app__header');
      expect(appHeader).toBeInTheDocument();
      expect(appHeader?.tagName.toLowerCase()).toBe('header');
    });
  });
});
