import type { Diagnostic } from '@codemirror/lint';
import { atom } from 'jotai';
import { diagnosticsAtom } from '../atoms';

export const CodeMirrorDiagnosticsAtom = atom((get) => {
  const diags = get(diagnosticsAtom);
  return diags.map((d): Diagnostic => {
    const start = d.start_ch ?? 0;
    const end = d.end_ch ?? 0;
    return {
      from: start,
      to: start === end ? end + 1 : end,
      message: d.message,
      severity: d.type === 'warning' ? 'warning' : 'error',
      source: 'baml',
      markClass:
        d.type === 'error'
          ? 'decoration-wavy decoration-red-500 text-red-450 stroke-blue-500'
          : 'decoration-wavy decoration-yellow-500 text-yellow-450 stroke-blue-500',
    };
  });
});
