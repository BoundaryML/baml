import type { ChangeEvent, CSSProperties, FC } from 'react';
import { useEffect, useMemo, useRef, useState } from 'react';
import initWasm, { BamlWasmRuntime, version, hotReloadTestString } from '@b/bridge_wasm';
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
  const runtimeRef = useRef<BamlWasmRuntime | null>(null);
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
          setHotReloadTestStr(hotReloadTestString());
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

    if (!isReady || !runtimeRef.current) {
      return;
    }

    try {
      runtimeRef.current.setSource(code);
      const names = runtimeRef.current.functionNames();
      setFunctionNames(names);
      setError(null);
    } catch (e) {
      console.error('Failed to update or get function names:', e);
      setError(e instanceof Error ? e.message : String(e));
      setFunctionNames([]);
    }
  }, [code, isReady]);

  useEffect(() => {
    let cancelled = false;
    const rootPath = '/baml_src';

    initWasm()
      .then(() => {
        if (cancelled) {
          return;
        }

        const srcFilesJson = JSON.stringify({ 'main.baml': latestCodeRef.current });
        const noopFetch = async (
          _method: string,
          _url: string,
          _headersJson: string,
          _body: string
        ): Promise<{ status: number; headersJson: string; url: string; bodyPromise: Promise<string> }> => ({
          status: 500,
          headersJson: '{}',
          url: '',
          bodyPromise: Promise.resolve('')
        });
        const runtime = BamlWasmRuntime.create(rootPath, srcFilesJson, noopFetch);
        runtimeRef.current = runtime;

        try {
          const names = runtime.functionNames();
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
      runtimeRef.current?.free();
      runtimeRef.current = null;
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
