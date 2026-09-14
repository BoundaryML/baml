import { defineConfig } from 'prisma/config';
import { optionalPostgresConnectionUrl } from './src/database-url.js';

const url = optionalPostgresConnectionUrl(process.env);

export default defineConfig({
  migrations: {
    path: 'prisma/migrations',
  },
  schema: 'prisma/schema.prisma',
  ...(url ? { datasource: { url } } : {}),
});
