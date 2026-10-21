import { PrismaPg } from '@prisma/adapter-pg';
import { postgresConnectionUrl } from './database-url.js';
import { PrismaClient } from './generated/prisma/client.js';

let client: PrismaClient | undefined;

export function database(environment = process.env): PrismaClient {
  if (client) return client;
  const adapter = new PrismaPg({
    connectionString: postgresConnectionUrl(environment),
  });
  client = new PrismaClient({ adapter });
  return client;
}

export async function disconnectDatabase(): Promise<void> {
  if (!client) return;
  const activeClient = client;
  client = undefined;
  await activeClient.$disconnect();
}
