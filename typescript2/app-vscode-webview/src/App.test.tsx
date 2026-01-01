import { describe, it, expect } from 'vitest';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import App from './App';

describe('App', () => {
  describe('initial render', () => {
    it('should render the main app container', () => {
      render(<App />);

      const main = screen.getByRole('main');
      expect(main).toBeInTheDocument();
      expect(main).toHaveClass('app');
    });

    it('should have injected-hot-reload4 in the DOM', async () => {
      render(<App />);

      // injected-hot-reload4 is added by the WASM function_names() and rendered as a function name
      await waitFor(() => {
        expect(screen.getByText('injected-hot-reload4()')).toBeInTheDocument();
      });
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

  describe('SplitPreview component integration', () => {
    it('should render the Editor panel', async () => {
      render(<App />);

      await waitFor(() => {
        expect(screen.getByText('Editor')).toBeInTheDocument();
      });
    });

    it('should render the Functions section header', async () => {
      render(<App />);

      await waitFor(() => {
        expect(screen.getByText('Functions (via Salsa)')).toBeInTheDocument();
      });
    });

    it('should render the Casing Variants section header', async () => {
      render(<App />);

      await waitFor(() => {
        expect(screen.getByText('Casing Variants')).toBeInTheDocument();
      });
    });

    it('should render the textarea editor', async () => {
      render(<App />);

      await waitFor(() => {
        const textarea = screen.getByRole('textbox');
        expect(textarea).toBeInTheDocument();
      });
    });

    it('should display default BAML code in the editor', async () => {
      render(<App />);

      await waitFor(() => {
        const textarea = screen.getByRole('textbox') as HTMLTextAreaElement;
        expect(textarea.value).toContain('function assertOk');
      });
    });

    it('should display parsed function names after WASM loads', async () => {
      render(<App />);

      // Wait for WASM to initialize and parse the default code
      await waitFor(
        () => {
          // The default code has assertOk, assertNotOk, assertBool functions
          expect(screen.getByText('assertOk()')).toBeInTheDocument();
        },
        { timeout: 2000 }
      );

      expect(screen.getByText('assertNotOk()')).toBeInTheDocument();
      expect(screen.getByText('assertBool()')).toBeInTheDocument();
    });

    it('should display casing variant labels after WASM loads', async () => {
      render(<App />);

      // Wait for WASM to initialize and render variants
      await waitFor(
        () => {
          expect(screen.getByText('Original')).toBeInTheDocument();
        },
        { timeout: 2000 }
      );

      // Check all variant labels are present (some appear multiple times as both label and value)
      expect(screen.getByText('lower')).toBeInTheDocument();
      expect(screen.getByText('UPPER')).toBeInTheDocument();
      // camelCase, PascalCase, snake_case, etc. appear as both labels and mock values
      expect(screen.getAllByText('camelCase').length).toBeGreaterThan(0);
      expect(screen.getAllByText('PascalCase').length).toBeGreaterThan(0);
      expect(screen.getAllByText('snake_case').length).toBeGreaterThan(0);
      expect(screen.getAllByText('UPPER_SNAKE').length).toBeGreaterThan(0);
      expect(screen.getAllByText('kebab-case').length).toBeGreaterThan(0);
      expect(screen.getAllByText('Title Case').length).toBeGreaterThan(0);
    });
  });

  describe('user interactions', () => {
    it('should update editor content when user types', async () => {
      const user = userEvent.setup();
      render(<App />);

      await waitFor(() => {
        expect(screen.getByRole('textbox')).toBeInTheDocument();
      });

      const textarea = screen.getByRole('textbox');

      // Clear and type new content
      await user.clear(textarea);
      await user.type(textarea, 'function NewFunc() -> int');

      expect(textarea).toHaveValue('function NewFunc() -> int');
    });

    it('should update function names when code changes', async () => {
      const user = userEvent.setup();
      render(<App />);

      await waitFor(() => {
        expect(screen.getByRole('textbox')).toBeInTheDocument();
      });

      const textarea = screen.getByRole('textbox');

      // Clear and type new function
      await user.clear(textarea);
      await user.type(textarea, 'function MyNewFunction() -> string');

      // Wait for the function name to appear
      await waitFor(() => {
        expect(screen.getByText('MyNewFunction()')).toBeInTheDocument();
      });
    });
  });

  describe('layout structure', () => {
    it('should have two article panels (editor and preview)', async () => {
      render(<App />);

      await waitFor(() => {
        const articles = screen.getAllByRole('article');
        expect(articles).toHaveLength(2);
      });
    });

    it('should have Editor panel as first article', async () => {
      render(<App />);

      await waitFor(() => {
        const articles = screen.getAllByRole('article');
        const editorPanel = articles[0];
        expect(within(editorPanel).getByText('Editor')).toBeInTheDocument();
        expect(within(editorPanel).getByRole('textbox')).toBeInTheDocument();
      });
    });

    it('should have preview panel as second article', async () => {
      render(<App />);

      await waitFor(() => {
        const articles = screen.getAllByRole('article');
        const previewPanel = articles[1];
        expect(within(previewPanel).getByText('Functions (via Salsa)')).toBeInTheDocument();
        expect(within(previewPanel).getByText('Casing Variants')).toBeInTheDocument();
      });
    });
  });
});

describe('App accessibility', () => {
  it('should have proper heading hierarchy', () => {
    render(<App />);

    const h1 = screen.getByRole('heading', { level: 1 });
    expect(h1).toBeInTheDocument();
  });

  it('should have accessible textarea with placeholder', async () => {
    render(<App />);

    await waitFor(() => {
      const textarea = screen.getByRole('textbox');
      expect(textarea).toHaveAttribute('placeholder');
    });
  });

  it('should have spellcheck disabled on code editor', async () => {
    render(<App />);

    await waitFor(() => {
      const textarea = screen.getByRole('textbox');
      expect(textarea).toHaveAttribute('spellcheck', 'false');
    });
  });
});
