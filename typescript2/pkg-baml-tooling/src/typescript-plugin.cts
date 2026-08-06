import type ts from 'typescript/lib/tsserverlibrary';

function init(_options: {
  typescript: typeof ts;
}): ts.server.PluginModule {
  throw new Error('not implemented');
}

export = init;
