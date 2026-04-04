import type { FC } from 'react';
import type { ParamInfo } from '../worker-protocol';
import { FieldRenderer } from './FieldRenderer';

interface ParameterFormProps {
  params: ParamInfo[];
  value: Record<string, unknown>;
  onChange: (value: Record<string, unknown>) => void;
}

export const ParameterForm: FC<ParameterFormProps> = ({ params, value, onChange }) => {
  const handleFieldChange = (paramName: string, fieldValue: unknown) => {
    onChange({ ...value, [paramName]: fieldValue });
  };

  if (params.length === 0) {
    return (
      <div className="px-3 py-2 text-[11px] text-vsc-text-faint">
        This function takes no parameters.
      </div>
    );
  }

  return (
    <div className="space-y-2 p-2">
      {params.map((param) => (
        <div key={param.name} className="space-y-0.5">
          <label className="block font-vsc-mono text-[11px] text-vsc-text-muted">
            {param.name}
          </label>
          <FieldRenderer
            fieldType={param.fieldType}
            value={value[param.name]}
            onChange={(v) => handleFieldChange(param.name, v)}
            depth={0}
            path={param.name}
          />
        </div>
      ))}
    </div>
  );
};
