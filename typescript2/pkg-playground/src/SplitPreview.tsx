import type { ChangeEvent, CSSProperties, FC } from 'react';
import { useEffect, useMemo, useRef, useState } from 'react';
import initWasm, { BamlProject, version, hot_reload_test_string } from '@b/baml-playground-wasm';
import { usePlayground } from './PlaygroundProvider';

const containerStyles: CSSProperties = {
  gridColumn: '1 / -1',
  display: 'grid',
  gridTemplateColumns: '1fr 1fr',
  gap: '1rem',
  width: '100%',
  minHeight: '320px'
};

const panelStyles: CSSProperties = {
  display: 'flex',
  flexDirection: 'column',
  borderRadius: '0.75rem',
  border: '1px solid rgba(15, 23, 42, 0.12)',
  background: '#ffffff',
  overflow: 'hidden'
};

const headerStyles: CSSProperties = {
  padding: '0.75rem 1rem',
  fontWeight: 600,
  borderBottom: '1px solid rgba(15, 23, 42, 0.08)',
  background: '#f8fafc'
};

const textareaStyles: CSSProperties = {
  flex: 1,
  padding: '1rem',
  fontFamily: '"Fira Code", "SFMono-Regular", Consolas, monospace',
  fontSize: '0.95rem',
  border: 'none',
  outline: 'none',
  resize: 'none'
};

const functionNamesContainerStyles: CSSProperties = {
  flex: 1,
  padding: '1rem',
  background: '#1e293b',
  color: '#e2e8f0'
};

const functionNameItemStyles: CSSProperties = {
  display: 'inline-block',
  margin: '0.25rem',
  padding: '0.5rem 0.75rem',
  borderRadius: '0.375rem',
  background: 'rgba(56, 189, 248, 0.15)',
  border: '1px solid rgba(56, 189, 248, 0.3)',
  fontFamily: '"Fira Code", "SFMono-Regular", Consolas, monospace',
  fontSize: '0.85rem',
  color: '#38bdf8'
};

const emptyFunctionsStyles: CSSProperties = {
  color: '#64748b',
  fontStyle: 'italic',
  fontSize: '0.9rem'
};

export const SplitPreview: FC = () => {
  const { code, setCode } = usePlayground();
  const projectRef = useRef<BamlProject | null>(null);
  const latestCodeRef = useRef<string>(code);
  const [functionNames, setFunctionNames] = useState<string[]>([]);
  const [isReady, setReady] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);
  const [hotReloadTestStr, setHotReloadTestStr] = useState<string | null>(null);

  // Inject version meta tag into document head
  useEffect(() => {
    let metaTag: HTMLMetaElement | null = null;

    initWasm()
      .then(() => {
        try {
          const ver = version();
          metaTag = document.createElement('meta');
          metaTag.name = 'baml-version';
          metaTag.content = ver;
          document.head.appendChild(metaTag);
        } catch (e) {
          console.error('Failed to get WASM version:', e);
        }
        try {
          setHotReloadTestStr(hot_reload_test_string());
        } catch (e) {
          console.error('Failed to get hot reload test string:', e);
        }
      })
      .catch((e) => {
        console.error('Failed to init WASM for version:', e);
      });

    return () => {
      if (metaTag) {
        document.head.removeChild(metaTag);
      }
    };
  }, []);

  useEffect(() => {
    latestCodeRef.current = code;

    if (!isReady || !projectRef.current) {
      return;
    }

    projectRef.current.set_source(code);

    // Get function names from the Salsa-backed query
    try {
      const names = projectRef.current.function_names();
      setFunctionNames(names);
    } catch (e) {
      console.error('Failed to get function names:', e);
      setFunctionNames([]);
    }
  }, [code, isReady]);

  useEffect(() => {
    let cancelled = false;

    initWasm()
      .then(() => {
        if (cancelled) {
          return;
        }

        const project = new BamlProject(latestCodeRef.current);
        projectRef.current = project;

        // Get initial function names
        try {
          const names = project.function_names();
          setFunctionNames(names);
        } catch (e) {
          console.error('Failed to get function names:', e);
          setFunctionNames([]);
        }

        setReady(true);
      })
      .catch((cause: unknown) => {
        if (cancelled) {
          return;
        }
        setError(cause instanceof Error ? cause.message : String(cause));
      });

    return () => {
      cancelled = true;
      projectRef.current?.free();
      projectRef.current = null;
    };
  }, []);

  const onChange = useMemo(
    () => (event: ChangeEvent<HTMLTextAreaElement>) => {
      setCode(event.target.value);
    },
    [setCode]
  );

  return (
    <section style={containerStyles}>
      {/* Hidden element for hot reload testing - see hot-reload.hmr.test.ts */}
      {hotReloadTestStr && (
        <span data-testid="hot-reload-test" style={{ display: 'none' }}>
          {hotReloadTestStr}
        </span>
      )}
      <article style={panelStyles}>
        <header style={headerStyles}>Editor</header>
        <textarea
          spellCheck={false}
          value={code}
          onChange={onChange}
          style={textareaStyles}
          placeholder="Start typing BAML here, e.g.:&#10;&#10;function MyFunction(input: string) -> string {&#10;  // function body&#10;}"
        />
      </article>
      <article style={panelStyles}>
        <header style={headerStyles}>Functions (via Salsa)</header>
        <div style={functionNamesContainerStyles}>
          {error ? (
            <span style={emptyFunctionsStyles}>Unable to parse functions</span>
          ) : functionNames.length > 0 ? (
            functionNames.map((name) => (
              <span key={name} style={functionNameItemStyles}>
                {name}()
              </span>
            ))
          ) : (
            <span style={emptyFunctionsStyles}>
              No functions defined. Try adding: function MyFunc(x: int) -&gt; string
            </span>
          )}
        </div>
      </article>
    </section>
  );
};
