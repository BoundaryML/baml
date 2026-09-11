import { resolve } from 'node:path';

import { validateBuiltStyles } from '../lib/built-styles';

await validateBuiltStyles(resolve(import.meta.dirname, '../.next'));
console.log('Validated the book page’s compiled stylesheets.');
