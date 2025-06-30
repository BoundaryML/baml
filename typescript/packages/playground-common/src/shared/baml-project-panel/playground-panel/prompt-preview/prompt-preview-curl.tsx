import { useAtomValue } from 'jotai';
import { atom } from 'jotai';
import { loadable } from 'jotai/utils';
import { ctxAtom, envVarsAtom, runtimeAtom } from '../../atoms';
import { selectionAtom } from '../atoms';
import { Loader } from './components';
import { ErrorMessage } from './components';
import { WithCopyButton } from './components';
import { findMediaFile } from './media-utils';

type CurlResult = string | undefined | Error;

const baseCurlAtom = atom<Promise<CurlResult>>(async (get) => {
  const rt = get(runtimeAtom).rt;
  const ctx = get(ctxAtom);
  const envVars = get(envVarsAtom);
  const { selectedFn, selectedTc } = get(selectionAtom);

  if (!selectedFn || !rt || !selectedTc || !ctx) {
    return undefined;
  }

  try {
    return await selectedFn.render_raw_curl_for_test(
      rt,
      selectedTc.name,
      ctx,
      false,
      false,
      findMediaFile,
      envVars,
    );
  } catch (error) {
    return error as Error;
  }
});

const curlAtom = loadable(baseCurlAtom);
export const PromptPreviewCurl = () => {
  const curl = useAtomValue(curlAtom);

  if (curl.state === 'loading') {
    return <Loader />;
  }

  if (curl.state === 'hasError') {
    return (
      <ErrorMessage error={JSON.stringify(curl.error) || 'Unknown error'} />
    );
  }

  const value = curl.data;
  if (value === undefined) {
    return null;
  }

  if (value instanceof Error) {
    return <ErrorMessage error={value.message || 'Unknown error'} />;
  }
  return (
    <WithCopyButton text={value}>
      <pre className="w-full whitespace-pre-wrap break-all rounded-lg border bg-muted p-4 font-mono text-xs">
        {value}
      </pre>
    </WithCopyButton>
  );
};
