import { loader } from 'fumadocs-core/source';
import { docs } from '@/.source/server';

export const authoredSource = loader({
  baseUrl: '/',
  source: docs.toFumadocsSource(),
});
