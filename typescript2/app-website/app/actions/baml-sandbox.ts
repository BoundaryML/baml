/* eslint-disable @typescript-eslint/require-await */
/* eslint-disable @typescript-eslint/no-floating-promises */
'use server';
import path from 'path';
import {
  type ExecutionError,
  type OutputMessage,
  Sandbox,
} from '@e2b/code-interpreter';
import { createStreamableValue } from 'ai/rsc';
import { type ICodeBlock } from '@/lib/mdx/baml-block/baml-project-panel/types';

export interface OutputLine {
  error: boolean;
  line: string;
  timestamp: number;
}

const TS_TEMPLATE = 'lon4mz4ocbc5gq50hnqc';
const PY_TEMPLATE = '30pqusah0dialhijz2c1';

export const runCode = async (
  code: ICodeBlock,
  files: { path: string; content: string }[],
) => {
  const stream = createStreamableValue<OutputLine>();
  (async () => {
    try {
      // Aaron created this template sandbox from a dockerfile fyi.
      // baml version 0.67.0
      const sandbox = await Sandbox.create(
        code.language === 'typescript' ? TS_TEMPLATE : PY_TEMPLATE,
        {
          timeoutMs: 5_000,
          envs: {
            BAML_LOG: 'info',
            BOUNDARY_PROXY_URL: 'https://fiddle-proxy.fly.dev',
          },
        },
      );

      const bamlClientDir = '/home/user/baml_client';
      await sandbox.files.makeDir(bamlClientDir);
      console.log('Writing files to sandbox');
      await Promise.all(
        files.map((file) =>
          sandbox.files.write(
            path.join(bamlClientDir, file.path),
            file.content,
          ),
        ),
      );

      // const sandboxFiles = await sandbox.files.list(bamlClientDir);
      console.log('Files written to sandbox');

      if (code.language === 'typescript') {
        await sandbox.files.write('/home/user/main.ts', code.code);
        const res = await sandbox.commands.run('tsx main.ts', {
          cwd: '/home/user',
          onStdout: (output: string) => {
            console.log('outputttt', output);
            stream.update({
              error: false,
              line: output,
              timestamp: Date.now() * 1000,
            });
          },
          onStderr: (output: string) => {
            console.log('stderrrrrr', output);
            stream.update({
              error: true,
              line: output,
              timestamp: Date.now() * 1000,
            });
          },
        });
        console.log('res', res);
      } else {
        console.log('running python code');
        const res = await sandbox.runCode(code.code, {
          // language: code.language.replace("py", "python"),

          onStdout: (output: OutputMessage) => {
            console.log('output', output);
            stream.update({
              error: output.error,
              line: output.line,
              timestamp: output.timestamp,
            });
          },
          onStderr: (output: OutputMessage) => {
            console.log('stderr', output);
            stream.update({
              error: output.error,
              line: output.line,
              timestamp: output.timestamp,
            });
          },
          onError: (error: ExecutionError) => {
            console.log('error', error);
            stream.update({
              error: true,
              line: error.value,
              // note the traceback has more info but it seems it leaks env vars right now
              // error.traceback
              timestamp: Date.now() * 1000,
            });
          },
        });
        console.log('res', res);
      }
      stream.done();
      await sandbox.kill();
    } catch (error) {
      stream.update({
        error: true,
        line: error instanceof Error ? error.message : 'Unknown error occurred',
        timestamp: Date.now() * 1000,
      });
      stream.done();
    }
  })();

  return { object: stream.value };
};
