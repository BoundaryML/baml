import type { Diagnostic } from '@codemirror/lint';
import { atom } from 'jotai';
import { diagnosticsAtom } from '../atoms';

export const CodeMirrorDiagnosticsAtom = atom((get) => {
  const diags = get(diagnosticsAtom);
  return diags.map((d): Diagnostic => {
    return {
      from: d.start_ch,
      to: d.start_ch === d.end_ch ? d.end_ch + 1 : d.end_ch,
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
