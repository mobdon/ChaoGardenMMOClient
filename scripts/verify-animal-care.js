import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdir, readFile, stat, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { detectGameFiles } from './game-files.js';

const game = await detectGameFiles(process.argv[2]);
if (!game.found) throw new Error('Installed Sonic Adventure2 required.');
const data = JSON.parse(await readFile('.local/care/animals.json', 'utf8'));
const manifest = JSON.parse(await readFile('.local/extracted/care/manifest.json', 'utf8'));
const digest = bytes => createHash('sha256').update(bytes).digest('hex');
const binary = await readFile(path.join(game.gamePath, 'sonic2app.exe'));
const pe = binary.readUInt32LE(60), optional = pe + 24, imageBase = binary.readUInt32LE(optional + 28);
const sectionStart = optional + binary.readUInt16LE(pe + 20);
const zeroFill = (address, length) => {
  const rva = address - imageBase;
  for (let i = 0; i < binary.readUInt16LE(pe + 6); i++) {
    const at = sectionStart + i * 40, start = binary.readUInt32LE(at + 12), size = binary.readUInt32LE(at + 16), virtualSize = binary.readUInt32LE(at + 8);
    if (rva >= start + size && rva + length <= start + virtualSize) return true;
  }
  return false;
};
const toFile = (address, length) => {
  const rva = address - imageBase;
  for (let i = 0; i < binary.readUInt16LE(pe + 6); i++) {
    const at = sectionStart + i * 40, start = binary.readUInt32LE(at + 12), size = binary.readUInt32LE(at + 16);
    if (rva >= start && rva + length <= start + size) return binary.readUInt32LE(at + 20) + rva - start;
  }
  return null;
};
const fields = ['mood', 'belly', 'swim', 'fly', 'run', 'power', 'stamina', 'luck', 'intelligence', 'unknown18'];
function parseStatsINI(text) {
  const rows = [];
  for (const block of text.split(/(?=^\[\d+\])/m)) {
    const id = block.match(/^\[(\d+)\]/);
    if (id) rows[Number(id[1])] = Object.fromEntries([...block.matchAll(/^(\w+)=(-?\d+)\s*$/gm)].map(match => [match[1].toLowerCase(), Number(match[2])]));
  }
  return rows;
}
const splitAnimalStats = parseStatsINI(await readFile('.local/extracted/care/Chao/AnimalAndDriveStats.ini', 'utf8'));
const splitFruitStats = parseStatsINI(await readFile('.local/extracted/care/Chao/FruitStats.ini', 'utf8'));
function verifyStats(rows, splitRows, start, count) {
  assert.equal(rows.length, count);
  assert.equal(splitRows.length, count);
  for (let index = 0; index < count; index++) {
    const values = rows[index].values || rows[index].nativeItemStats;
    const at = toFile(start + index * 20, 20);
    assert.notEqual(at, null);
    fields.forEach((field, fieldIndex) => {
      assert.equal(values[field], binary.readInt16LE(at + fieldIndex * 2), `Native signed field mismatch: ${index}/${field}`);
      if (field !== 'unknown18') assert.equal(values[field], splitRows[index][field], `Independent SA Tools mismatch: ${index}/${field}`);
    });
  }
}
assert.equal(data.animals.length, 21);
assert.equal(data.drives.length, 4);
assert.deepEqual(data.animals.map(item => item.id), Array.from({ length: 21 }, (_, i) => i));
assert.deepEqual(data.drives.map(item => item.id), [21,22,23,24]);
assert.equal(data.reservedStatRows[0].index, 25);
verifyStats([...data.animals, ...data.drives, ...data.reservedStatRows], splitAnimalStats, 0x008a6240, 26);
verifyStats(data.fruitStats, splitFruitStats, 0x008a6448, 24);
let partSlotCount = 0, attachedPartCount = 0, mappedPartCount = 0, ambiguousPartCount = 0, runtimePartLists = 0, zeroInitializedPartLists = 0;
for (const animal of data.animals) {
  const modelPointerAt = toFile(0x012966f0 + animal.id * 4, 4);
  assert.equal(Number.parseInt(animal.source.modelPointer), binary.readUInt32LE(modelPointerAt));
  assert.equal(Number.parseInt(animal.nativeHistoryBitHex), 2 ** animal.id);
  for (const [age, table] of [['child', 0x012e59a0], ['adult', 0x012e5938]]) {
    const record = animal.parts[age], pointer = binary.readUInt32LE(toFile(table + animal.id * 4, 4));
    assert.equal(Number.parseInt(record.pointer), pointer);
    const listAt = toFile(pointer, 52);
    if (listAt === null && zeroFill(pointer, 52)) {
      assert.equal(record.status, 'zero-initialized-empty-list');
      assert.equal(record.mapped, true);
      assert.deepEqual(record.slots.map(slot => [slot.pointer, slot.model]), Array.from({ length: 13 }, () => [null, null]));
      zeroInitializedPartLists++;
      continue;
    }
    if (listAt === null) {
      assert.equal(record.status, 'runtime-list-not-file-backed');
      assert.equal(record.mapped, false);
      assert.deepEqual(record.slots, []);
      runtimePartLists++;
      continue;
    }
    assert.equal(record.slots.length, 13);
    for (const [slotIndex, slot] of record.slots.entries()) {
      const native = binary.readUInt32LE(listAt + slotIndex * 4);
      assert.equal(slot.pointer ? Number.parseInt(slot.pointer) : 0, native);
      partSlotCount++;
      if (!native) { assert.equal(slot.model, null); continue; }
      attachedPartCount++;
      if (slot.candidates.length) ambiguousPartCount++;
      const models = slot.model ? [slot.model] : slot.candidates;
      for (const model of models) {
        let object = Number.parseInt(model.rootAddress);
        for (const step of model.hierarchyPath.split('/').slice(1)) {
          assert.ok(['child','sibling'].includes(step));
          object = binary.readUInt32LE(toFile(object,52) + (step === 'child' ? 44 : 48));
        }
        assert.equal(object, native, 'Mapped resource must actually contain the native object.');
        assert.ok((await stat(model.path)).size > 0);
      }
      if (slot.mapped) { assert.ok(models.length > 0); mappedPartCount++; }
    }
  }
}
assert.deepEqual(data.partHierarchyGaps, [], 'Every mapped part hierarchy must have a valid object layout.');
assert.equal(mappedPartCount, attachedPartCount, 'Every non-null native part slot must map to an extracted model.');
assert.equal(runtimePartLists, 0);
for (const drive of data.drives) {
  assert.equal(drive.grantsAnimalHistoryBit, false);
  assert.equal(drive.animalParts, false);
}
const config = await readFile('.local/extracted/care/care.ini', 'utf8');
const extractedFiles = [...config.matchAll(/^filename=(.+)$/gm)].map(match => path.resolve('.local/extracted/care', match[1].trim()));
for (const file of extractedFiles) assert.ok((await stat(file)).size > 0, `Missing extracted resource: ${file}`);
for (const correction of data.addressCorrections || []) {
  const at = toFile(Number.parseInt(correction.corrected, 16) + imageBase, 52);
  assert.ok(at !== null && binary.readUInt32LE(at) <= 0xffff, `Corrected address must hold an object: ${correction.name}`);
  const section = config.split(/(?=^\[)/m).find(block => block.startsWith(`[${correction.name}]`));
  assert.match(section || '', new RegExp(`^address=${correction.corrected}$`, 'm'), `care.ini must use the corrected address: ${correction.name}`);
}
for (const source of manifest.sources) {
  const bytes = await readFile(path.join(game.gamePath, source.relativePath));
  assert.equal(bytes.length, source.bytes);
  assert.equal(digest(bytes), source.sha256, `Original source changed: ${source.relativePath}`);
  assert.equal(source.unchanged, true);
}
assert.ok(data.animals.some(item => ['swim','fly','run','power'].some(field => item.nativeItemStats[field] < 0)), 'Negative native effects must survive signed decoding.');
const report = {
  version: 1, passed: true, originalsUnchanged: manifest.sources.length,
  animals: data.animals.length, drives: data.drives.length, nativeStatRows: 26, fruitStatRows:24,
  independentSAtoolsStatRowsMatched: 50, extractedResources: extractedFiles.length,
  animalModels: manifest.animalModels, animalMotionSections: manifest.animalMotionSections, animalPartModels: manifest.animalPartModels,
  fileBackedPartSlots: partSlotCount, nonzeroPartSlots: attachedPartCount, mappedPartSlots: mappedPartCount,
  ambiguousPartSlots: ambiguousPartCount, runtimePartLists, zeroInitializedPartLists, addressCorrections: data.addressCorrections || [], partHierarchyGaps: data.partHierarchyGaps,
  limits: ['No item-application formula or live care action claimed.', 'Half Fish, Skeleton Dog and Bat part lists are the shared zero-filled list (no parts); cross-checked against GameCube ChaoMain.rel, not observed at run time.', 'No animal GLBs, learned-behavior grants or Chaos evolution formula validated by this check.'],
};
await mkdir('.local/care', { recursive:true });
await writeFile('.local/care/verification-report.json', `${JSON.stringify(report,null,2)}\n`);
console.log(JSON.stringify(report,null,2));
