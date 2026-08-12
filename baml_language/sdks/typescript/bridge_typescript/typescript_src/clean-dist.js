// clean-dist.js - remove generated package artifacts before rebuilding.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const packageRoot = path.join(__dirname, '..');

fs.rmSync(path.join(packageRoot, 'dist'), { recursive: true, force: true });
fs.mkdirSync(path.join(packageRoot, 'dist'), { recursive: true });
