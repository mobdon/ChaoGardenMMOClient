import test from 'node:test';
import assert from 'node:assert/strict';
import { createSaveSlots, SESSION_KEY, SAVE_SLOTS_KEY } from '../src/save-slots.js';

function storage(values = {}) {
  const state = new Map(Object.entries(values));
  return { getItem: key => state.get(key) ?? null, setItem: (key, value) => state.set(key, value) };
}
const ownerToken = 'a'.repeat(64);
const otherToken = 'b'.repeat(64);
const owner = { id: 'owner', name: 'Chaos', form: 'chaos', admin: true, rings: 46, alignment: -100 };

test('legacy owner credential survives a new Chao and an unreadable slot index', () => {
  const data = storage({ [SESSION_KEY]: ownerToken, [SAVE_SLOTS_KEY]: '{broken' });
  const slots = createSaveSlots(data);
  assert.equal(slots.list()[0].token, ownerToken);
  slots.record(otherToken, { ...owner, id: 'friend', name: 'Friend', admin: false });
  assert.deepEqual(new Set(slots.list().map(row => row.token)), new Set([ownerToken, otherToken]));
  slots.record(ownerToken, owner);
  assert.equal(slots.list().find(row => row.token === ownerToken).name, 'Chaos');
  assert.equal(slots.list().length, 2);
});

test('ordinary snapshots never invent a durable save acknowledgement', () => {
  const slots = createSaveSlots(storage());
  assert.equal(slots.record(ownerToken, owner).savedAt, undefined);
  slots.record(ownerToken, owner, 123456);
  assert.equal(slots.record(ownerToken, { ...owner, rings: 50 }).savedAt, 123456);
});

test('a slot remembers the memory card its Chao is on', () => {
  const store = new Map();
  const storage = { getItem: k => store.get(k) ?? null, setItem: (k, v) => store.set(k, v) };
  const slots = createSaveSlots(storage);
  const token = 'a'.repeat(64), card = 'b'.repeat(64);
  slots.record(token, { id: 'one', name: 'One' }, undefined, card);
  assert.equal(slots.list()[0].card, card);
  slots.record(token, { id: 'one', name: 'One' }); // later saves keep the card
  assert.equal(slots.list()[0].card, card);
});

test('a fresh login of the same Chao keeps one slot', () => {
  const data = storage({ [SAVE_SLOTS_KEY]: JSON.stringify([1, 2, 3].map(n => ({ token: String(n).repeat(64), id: 'owner', name: 'Chaos', lastPlayedAt: n }))) });
  const slots = createSaveSlots(data);
  assert.deepEqual(slots.list().map(row => row.token), ['3'.repeat(64)]);
  slots.record(ownerToken, owner);
  assert.equal(JSON.parse(data.getItem(SAVE_SLOTS_KEY)).length, 1);
});
