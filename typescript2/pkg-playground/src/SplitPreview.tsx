import type { ChangeEvent, CSSProperties, FC } from 'react';
import { useEffect, useMemo, useRef, useState } from 'react';
import initWasm, { BamlRuntime, CasingVariants } from 'baml-runtime-wasm';
import { usePlayground } from './PlaygroundProvider';

type VariantKey =
  | 'original'
  | 'lower'
  | 'upper'
  | 'camel'
  | 'pascal'
  | 'snake'
  | 'upper_snake'
  | 'kebab'
  | 'title';

type RenderedVariants = Record<VariantKey, string>;

const VARIANT_DISPLAY: Array<{ key: VariantKey; label: string }> = [
  { key: 'original', label: 'Original' },
  { key: 'lower', label: 'lower' },
  { key: 'upper', label: 'UPPER' },
  { key: 'camel', label: 'camelCase' },
  { key: 'pascal', label: 'PascalCase' },
  { key: 'snake', label: 'snake_case' },
  { key: 'upper_snake', label: 'UPPER_SNAKE' },
  { key: 'kebab', label: 'kebab-case' },
  { key: 'title', label: 'Title Case' }
];

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

const variantsGridStyles: CSSProperties = {
  flex: 1,
  display: 'grid',
  gap: '0.75rem',
  padding: '1rem',
  background: '#0f172a',
  color: '#e2e8f0',
  overflowY: 'auto'
};

const variantRowStyles: CSSProperties = {
  display: 'flex',
  flexDirection: 'column',
  gap: '0.4rem'
};

const variantLabelStyles: CSSProperties = {
  fontSize: '0.75rem',
  textTransform: 'uppercase',
  letterSpacing: '0.1em',
  color: '#38bdf8'
};

const variantValueStyles: CSSProperties = {
  margin: 0,
  padding: '0.75rem',
  borderRadius: '0.5rem',
  background: 'rgba(15, 23, 42, 0.55)',
  border: '1px solid rgba(148, 163, 184, 0.25)',
  fontFamily: '"Fira Code", "SFMono-Regular", Consolas, monospace',
  fontSize: '0.92rem',
  whiteSpace: 'pre-wrap',
  wordBreak: 'break-word'
};

const placeholderValue = '// Rendered casing variants will appear here';

const extractVariants = (result: CasingVariants): RenderedVariants => {
  try {
    return {
      original: result.original,
      lower: result.lower,
      upper: result.upper,
      camel: result.camel,
      pascal: result.pascal,
      snake: result.snake,
      upper_snake: result.upper_snake,
      kebab: result.kebab,
      title: result.title
    };
  } finally {
    result.free();
  }
};

const functionNamesHeaderStyles: CSSProperties = {
  padding: '0.75rem 1rem',
  fontWeight: 600,
  borderBottom: '1px solid rgba(15, 23, 42, 0.08)',
  borderTop: '1px solid rgba(15, 23, 42, 0.08)',
  background: '#f8fafc'
};

const functionNamesContainerStyles: CSSProperties = {
  padding: '1rem',
  background: '#1e293b',
  color: '#e2e8f0',
  minHeight: '120px'
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
  const runtimeRef = useRef<BamlRuntime | null>(null);
  const latestCodeRef = useRef<string>(code);
  const [rendered, setRendered] = useState<RenderedVariants | null>(null);
  const [functionNames, setFunctionNames] = useState<string[]>([]);
  const [isReady, setReady] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    latestCodeRef.current = code;

    if (!isReady || !runtimeRef.current) {
      return;
    }

    runtimeRef.current.set_source(code);
    setRendered(extractVariants(runtimeRef.current.render()));

    // Get function names from the Salsa-backed query
    try {
      const names = runtimeRef.current.function_names();
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
        const runtime = new BamlRuntime(latestCodeRef.current);
        runtimeRef.current = runtime;
        setRendered(extractVariants(runtime.render()));

        // Get initial function names
        try {
          const names = runtime.function_names();
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
        {/* Function Names Section - Salsa-backed query results */}
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

        <header style={functionNamesHeaderStyles}>Casing Variants</header>
        <div style={variantsGridStyles}>
          {error ? (
            <pre style={variantValueStyles}>{`// Failed to load BAML runtime\n${error}`}</pre>
          ) : rendered ? (
            VARIANT_DISPLAY.map(({ key, label }) => (
              <div style={variantRowStyles} key={key}>
                <span style={variantLabelStyles}>{label}</span>
                <pre style={variantValueStyles}>{rendered[key]}</pre>
              </div>
            ))
          ) : (
            <pre style={variantValueStyles}>{placeholderValue}</pre>
          )}
        </div>
      </article>
    </section>
  );
};
