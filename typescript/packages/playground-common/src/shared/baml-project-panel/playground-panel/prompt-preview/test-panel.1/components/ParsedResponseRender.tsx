import { ScrollArea } from '@baml/ui/scroll-area';
import type {
  WasmFunctionResponse,
  WasmTestResponse,
} from '@gloo-ai/baml-schema-wasm-web';
import JsonView from '@uiw/react-json-view';
import { githubLightTheme as lightTheme } from '@uiw/react-json-view/githubLight';
import { vscodeTheme as darkTheme } from '@uiw/react-json-view/vscode';
import { useTheme } from 'next-themes';
import { RenderPromptPart } from '../../render-text';

const ErrorText = ({ text }: { text: string }) => {
  return <pre className="text-xs text-red-500 whitespace-pre-wrap">{text}</pre>;
};

// Renders the parsed response only
export const ParsedResponseRenderer: React.FC<{
  response?: WasmFunctionResponse | WasmTestResponse;
}> = ({ response }) => {
  if (!response) {
    return (
      <div className="text-xs text-muted-foreground">
        Waiting for response...
      </div>
    );
  }

  const llmFailure = response.llm_failure();
  const failureMessage =
    'failure_message' in response ? response.failure_message() : undefined;

  if (llmFailure) {
    return <ErrorText text={llmFailure.message} />;
  }

  const parsedResponse = response.parsed_response();

  return (
    <div>
      {typeof parsedResponse === 'string' ? (
        <ParsedResponseRender response={parsedResponse} />
      ) : (
        <ParsedResponseRender response={parsedResponse?.value} />
      )}
      {failureMessage && <ErrorText text={failureMessage} />}
    </div>
  );
};

const ParsedResponseRender = ({
  response,
}: { response: string | undefined }) => {
  const { theme } = useTheme();

  if (!response) {
    return null;
  }

  let parsedResponseObj;
  try {
    parsedResponseObj = JSON.parse(response);
    if (parsedResponseObj === null || typeof parsedResponseObj !== 'object') {
      return <RenderPromptPart text={response} />;
    }
  } catch (e) {
    parsedResponseObj = response;
  }

  if (typeof parsedResponseObj === 'string') {
    return <RenderPromptPart text={parsedResponseObj} />;
  }

  return (
    <div className="flex h-full text-xs">
      <ScrollArea className="pr-2 w-full text-xs" type="always">
        <JsonView
          className="p-1 w-full rounded-md"
          value={parsedResponseObj || {}}
          collapsed={false}
          enableClipboard={true}
          displayDataTypes={false}
          displayObjectSize={true}
          indentWidth={16}
          shortenTextAfterLength={700}
          style={theme === 'dark' ? darkTheme : lightTheme}
        >
          <JsonView.String
            render={({ children, ...reset }, { type, value, keyName }) => {
              if (type === 'type') {
                return <span />;
              }
              if (type === 'value') {
                return (
                  <span {...reset} className="whitespace-pre-wrap break-all">
                    &quot;{children}&quot;
                    <span className="text-muted-foreground">, </span>
                  </span>
                );
              }
            }}
          />
          <JsonView.Colon
            render={(props, { parentValue, value, keyName }) => {
              if (Array.isArray(parentValue) && props.children == ':') {
                return <span />;
              }
              return <span {...props}>: </span>;
            }}
          />

          <JsonView.Null
            render={({ children, ...reset }) => (
              <span {...reset} className="whitespace-pre-wrap break-words">
                null
              </span>
            )}
          />
          <JsonView.Undefined
            render={({ children, ...reset }) => (
              <span {...reset} className="whitespace-pre-wrap break-words">
                undefined
              </span>
            )}
          />
          <JsonView.KeyName
            render={({ ...props }, { parentValue, value, keyName }) => {
              if (
                Array.isArray(parentValue) &&
                Number.isFinite(props.children)
              ) {
                return <span className="" />;
              }
              return <span className="" {...props} />;
            }}
          />
        </JsonView>
      </ScrollArea>
    </div>
  );
};
