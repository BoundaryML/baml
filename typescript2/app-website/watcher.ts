/* eslint-disable @typescript-eslint/require-await */
/* eslint-disable @typescript-eslint/no-misused-promises */

import { exec } from 'node:child_process';
import fs from 'node:fs';
import { promisify } from 'node:util';
import { WebSocketServer } from 'ws';

const execAsync = promisify(exec);

const CONTENT_FOLDER = 'posts';
const LESSONS_FOLDER = 'lessons';

// Watch posts folder
fs.watch(
  CONTENT_FOLDER,
  { persistent: true, recursive: true },
  async (eventType, fileName) => {
    clients.forEach((ws) => {
      // do any pre-processing you want to do here...
      ws.send('refresh');
    });
  },
);

// Watch lessons folder
fs.watch(
  LESSONS_FOLDER,
  { persistent: true, recursive: true },
  async (eventType, fileName) => {
    console.log(`Change detected in lessons: ${eventType} ${fileName}`);

    try {
      // Run generate-lessons.js script
      await execAsync('node scripts/generate-lessons.js');
      console.log('Lessons regenerated successfully');

      // Send refresh signal to all connected clients
      clients.forEach((ws) => {
        ws.send('refresh');
      });
    } catch (error) {
      console.error('Error regenerating lessons:', error);
    }
  },
);

const wss = new WebSocketServer({ port: 3201 });

const clients = new Set<any>();

wss.on('connection', function connection(ws) {
  clients.add(ws);
  ws.on('error', console.error);
  ws.on('close', () => clients.delete(ws));
});
