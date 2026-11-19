/** What a team wants to know about production, and what it takes to find out in the traditional
 *  stack. In Boundary each one is a single query. Plain data, so server and client components can
 *  both read it. */
export const ASKS = [
  { ask: 'Evals on real traffic', today: 'export, then join by hand' },
  { ask: 'Accuracy over time', today: 'only runs someone labeled' },
  { ask: 'Exactly what happened', today: 'sampled, across 3 tools' },
  { ask: 'Record and replay', today: 'only if you built it in ahead of time' },
  { ask: 'Cost per feature', today: 'prompts and users live apart' },
];
