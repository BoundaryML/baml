import React, { useCallback } from 'react';
import { Button } from '@baml/ui/button';
import { Input } from '@baml/ui/input';
import { Trash2 } from 'lucide-react';
import { useAtom } from 'jotai';
import { pendingApiKeyRowsAtom } from './atoms';

export const AddApiKeyForm: React.FC = () => {
  // Use jotai atom for rows state
  const [rows, setRows] = useAtom(pendingApiKeyRowsAtom);

  // Add a new empty row
  const handleAddRow = useCallback(() => {
    setRows((prev) => [...prev, { key: '', value: '' }]);
  }, [setRows]);

  // Remove a row by index
  const handleRemoveRow = useCallback((idx: number) => {
    setRows((prev) => prev.filter((_, i) => i !== idx));
  }, [setRows]);

  // Update a row's key or value
  const handleChange = useCallback((idx: number, field: 'key' | 'value', value: string) => {
    setRows((prev) =>
      prev.map((row, i) =>
        i === idx ? { ...row, [field]: value } : row
      )
    );
  }, [setRows]);

  // Render
  return (
    <>
      {/* Labels at the top */}
      <div className="grid grid-cols-[1fr_1fr_40px] gap-4 mb-1">
        <div className="text-xs font-semibold text-muted-foreground pl-1">Key</div>
        <div className="text-xs font-semibold text-muted-foreground pl-1">Value</div>
        <div></div>
      </div>
      {/* Rows */}
      <div className="flex flex-col gap-2">
        {rows.map((row, idx) => (
          <div className="grid grid-cols-[1fr_1fr_40px] gap-4 items-center" key={idx}>
            <Input
              id={`new-api-key-${idx}`}
              value={row.key}
              onChange={(e) => handleChange(idx, 'key', e.target.value)}
              placeholder="e.g. API_KEY"
              className="h-10 text-sm font-mono placeholder:font-sans"
              autoComplete="off"
            />
            <Input
              id={`new-api-key-value-${idx}`}
              value={row.value}
              onChange={(e) => handleChange(idx, 'value', e.target.value)}
              placeholder=""
              className="h-10 text-sm font-mono placeholder:font-sans"
              autoComplete="off"
            />
            <Button
              variant="ghost"
              size="icon"
              onClick={() => handleRemoveRow(idx)}
              type="button"
              aria-label="Remove row"
              className="justify-self-end"
              disabled={rows.length === 1}
            >
              <Trash2 className="w-5 h-5 text-muted-foreground hover:text-destructive" />
            </Button>
          </div>
        ))}
      </div>
      {/* Add Another button */}
      <div className="mt-2">
        <Button
          size="sm"
          variant="outline"
          onClick={handleAddRow}
          className="gap-2"
          type="button"
        >
          <span className="flex items-center"><svg className="w-5 h-5 mr-1" fill="none" stroke="currentColor" strokeWidth="2" viewBox="0 0 24 24"><path strokeLinecap="round" strokeLinejoin="round" d="M12 4v16m8-8H4" /></svg>Add Another</span>
        </Button>
      </div>
    </>
  );
};