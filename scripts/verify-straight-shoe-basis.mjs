// The former separate-left/right basis proof is superseded by the actual
// canonical-right mirror fit. Keep this opt-in entry point for existing local
// workflows, including its source-root and output flags. The shared verifier
// checks source model65/78, production mounting helpers, every mirrored boot/
// jet vertex, and all three forms' native Walk/Run/Skate trajectories.
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
if (!process.argv.includes('--output')) process.argv.push('--output', resolve(root, '.local/reviews/straight-shoe-basis-proof.json'));
await import('./verify-chao-footwear.mjs');
