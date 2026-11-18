import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { detectGameFiles } from './game-files.js';

const gamePath = process.argv[2];
const result = await detectGameFiles(gamePath);
const output = path.resolve('.local/game-inventory.json');
await mkdir(path.dirname(output), { recursive: true });
await writeFile(output, `${JSON.stringify(result, null, 2)}\n`, 'utf8');
console.log(result.found ? 'SA2 installation found.' : 'SA2 installation not found. Set SA2_GAME_PATH or pass the installation folder.');
console.log(`Resources: ${result.inventory.total}; Chao-related: ${result.inventory.chao}; audio: ${result.inventory.audio}`);
console.log(`BATTLE: ${result.battle.note}`);
console.log(`Browser imports: Chao ${result.imports.chao ? 'present' : 'pending'}, garden ${result.imports.garden ? 'present' : 'pending'}`);
console.log(`Local inventory: ${output}`);
if (!result.found) process.exitCode = 1;
