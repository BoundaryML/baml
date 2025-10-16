import React from 'react';
import { ApiKeyListItem } from './api-key-list-item';
import { useAtomValue } from 'jotai';
import { renderedApiKeysAtom } from './atoms';

export const ApiKeysList: React.FC = () => {
  const apiKeys = useAtomValue(renderedApiKeysAtom);

  const filteredKeys = apiKeys.filter(({ key }) => key !== 'BOUNDARY_PROXY_URL');
  console.log('ApiKeysList: Rendering with apiKeys:', apiKeys);
  console.log('ApiKeysList: Filtered keys (excluding BOUNDARY_PROXY_URL):', filteredKeys);
  console.log('ApiKeysList: Number of filtered keys:', filteredKeys.length);

  if (filteredKeys.length === 0) {
    console.log('ApiKeysList: No API keys to display');
    return (
      <div className="text-sm text-muted-foreground text-center py-4">
        No API keys configured. Add some keys above to get started.
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-2">
      {filteredKeys.map((apiKey) => (
        <ApiKeyListItem
          key={apiKey.key}
          apiKey={apiKey}
        />
      ))}
    </div>
  );
};