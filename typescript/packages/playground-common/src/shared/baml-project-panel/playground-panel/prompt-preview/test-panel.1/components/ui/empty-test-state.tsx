import { memo } from 'react';

export const EmptyTestState = memo(() => (
  <div className="p-4 text-muted-foreground">No tests running</div>
));

EmptyTestState.displayName = 'EmptyTestState';