import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const tools = path.resolve(process.env.SA_TOOLS_DIR || path.join(root, '.local/tools/sa-tools-786'));
const mappingRoot = path.join(tools, 'GameConfig/PC_SA2');
const iniSections = (text) => {
  const records = new Map(); let section;
  for (const line of text.split(/\r?\n/)) {
    const heading = line.match(/^\[([^\]]+)\]/);
    if (heading) { section = {}; records.set(heading[1], section); }
    else if (section) { const field = line.match(/^([^;=][^=]*)=(.*)$/); if (field) section[field[1].trim()] = field[2].trim(); }
  }
  return records;
};
const fieldPickups = iniSections(await readFile(path.join(mappingRoot, 'CommonObjects.ini'), 'utf8'));
const models = {};
for (const archive of ['sonicmdl', 'teriosmdl', 'twalkmdl', 'ewalkmdl', 'knuckmdl', 'rougemdl']) {
  models[archive] = new Map((await readFile(path.join(mappingRoot, `MDL/${archive.toUpperCase()}.ini`), 'utf8')).split(/\r?\n/).filter(line => /^\d+=/.test(line)).map(line => {
    const [nativeId, rest] = line.split(/=(.*)/s); const [file, name, texture, nativeName] = rest.split('|');
    return [Number(nativeId), { modelId: Number(nativeId), name, archive: `resource/gd_PC/${archive}.prs`, textureArchive: texture ? `resource/gd_PC/${texture.toLowerCase()}.prs` : null, nativeName, evidence: `PC_SA2/MDL/${archive.toUpperCase()}.ini` }];
  }));
}
// Effects are concise retail behavior descriptions. They are not promises
// that the standalone browser prototype implements those abilities.
const definitions = [
  ['sonic-light-shoes', 'Sonic', 'Light Shoes', 0, 'SonicLightShoes', 'sonicmdl', [13,14], 'Light Shoes', 'Light Dash along ring trails.', 'Ring-trail traversal challenge.'],
  ['sonic-ancient-light', 'Sonic', 'Ancient Light', 1, 'SonicAncientLight', null, [], null, 'Charged Light Attack chains through enemies.', 'Validated enemy-chain targeting action.'],
  ['sonic-magic-gloves', 'Sonic', 'Magic Gloves', 2, 'SonicMagicGloves', 'sonicmdl', [27], 'Magic Gloves', 'Turns an enemy into a throwable ball.', 'Capture-and-throw action with server-owned targets.'],
  ['sonic-flame-ring', 'Sonic', 'Flame Ring', 3, 'SonicFlameRing', 'sonicmdl', [25], "Sonic's Flame Ring", 'Somersault breaks iron containers.', 'Obstacle-specific fire roll.'],
  ['sonic-bounce-bracelet', 'Sonic', 'Bounce Bracelet', 4, 'SonicBounceBracelet', 'sonicmdl', [26], 'Bounce Bracelet', 'Enables repeated bounce jumps.', 'Bounded bounce traversal.'],
  ['sonic-mystic-melody', 'Sonic', 'Mystic Melody', 5, 'SonicMysticMelody', null, [], null, 'Activates shrine paths toward Lost Chao.', 'Shrine puzzle interaction.'],
  ['tails-booster', 'Tails', 'Booster', 6, 'TailsBooster', 'twalkmdl', [319,321], 'Booster', 'Lets the mech hover after jumping.', 'Mech hover equipment.'],
  ['tails-bazooka', 'Tails', 'Bazooka', 7, 'TailsBazooka', 'twalkmdl', [309], 'Bazooka', 'Shoots through iron containers.', 'Reinforced-obstacle projectile.'],
  ['tails-laser-blaster', 'Tails', 'Laser Blaster', 8, 'TailsLaserBlaster', 'twalkmdl', [306], "Tails' Laser Blaster", 'Upgrades lock-on weapon power.', 'Mech weapon progression.'],
  ['tails-mystic-melody', 'Tails', 'Mystic Melody', 9, 'TailsMysticMelody', null, [], null, 'Activates shrine paths toward Lost Chao.', 'Shrine puzzle interaction.'],
  ['knuckles-shovel-claw', 'Knuckles', 'Shovel Claw', 10, 'KnucklesShovelClaw', 'knuckmdl', [162,163], 'Shovel Claw', 'Enables digging.', 'Diggable mission surfaces and buried rewards.'],
  ['knuckles-sunglasses', 'Knuckles', 'Sunglasses', 11, 'KnucklesSunglasses', 'knuckmdl', [161], 'Sunglasses', 'Reveals otherwise hidden objects.', 'Hidden-object perception challenge.'],
  ['knuckles-hammer-gloves', 'Knuckles', 'Hammer Gloves', 12, 'KnucklesHammerGloves', 'knuckmdl', [164,165], 'Hammer Gloves', 'Punches break iron containers.', 'Obstacle-specific reinforced punch.'],
  ['knuckles-air-necklace', 'Knuckles', 'Air Necklace', 13, 'KnucklesAirNecklace', 'knuckmdl', [168], 'Air Necklace', 'Allows unlimited underwater breathing.', 'Server-controlled underwater breathing equipment.'],
  ['knuckles-mystic-melody', 'Knuckles', 'Mystic Melody', 14, 'KnucklesMysticMelody', null, [], null, 'Activates shrine paths toward Lost Chao.', 'Shrine puzzle interaction.'],
  ['shadow-air-shoes', 'Shadow', 'Air Shoes', 16, 'ShadowAirShoes', 'teriosmdl', [78,79], 'Air Shoes', 'Light Dash along ring trails.', 'Ring-trail traversal challenge; owner shoe cosmetics first.'],
  ['shadow-ancient-light', 'Shadow', 'Ancient Light', 17, 'ShadowAncientLight', null, [], null, 'Charged Light Attack chains through enemies.', 'Validated enemy-chain targeting action.'],
  ['shadow-flame-ring', 'Shadow', 'Flame Ring', 18, 'ShadowFlameRing', 'teriosmdl', [100], "Shadow's Flame Ring", 'Somersault breaks iron containers.', 'Obstacle-specific fire roll.'],
  ['shadow-mystic-melody', 'Shadow', 'Mystic Melody', 19, 'ShadowMysticMelody', null, [], null, 'Activates shrine paths toward Lost Chao.', 'Shrine puzzle interaction.'],
  ['eggman-jet-engine', 'Eggman', 'Jet Engine', 20, 'EggmanJetEngine', 'ewalkmdl', [274,276], 'Jet Engine', 'Lets the mech hover after jumping.', 'Mech hover equipment.'],
  ['eggman-large-cannon', 'Eggman', 'Large Cannon', 21, 'EggmanLargeCannon', 'ewalkmdl', [263], 'Large Cannon', 'Shoots through iron containers.', 'Reinforced-obstacle projectile.'],
  ['eggman-laser-blaster', 'Eggman', 'Laser Blaster', 22, 'EggmanLaserBlaster', 'ewalkmdl', [260], "Eggman's Laser Blaster", 'Upgrades lock-on weapon power.', 'Mech weapon progression.'],
  ['eggman-protective-armor', 'Eggman', 'Protective Armor', 23, 'EggmanProtectiveArmor', 'ewalkmdl', [278], 'Protection Armor', 'Increases maximum mech health.', 'Bounded mech health upgrade.'],
  ['eggman-mystic-melody', 'Eggman', 'Mystic Melody', 24, 'EggmanMysticMelody', null, [], null, 'Activates shrine paths toward Lost Chao.', 'Shrine puzzle interaction.'],
  ['rouge-pick-nails', 'Rouge', 'Pick Nails', 25, 'RougePickNails', 'rougemdl', [195,199], 'Pick Nails', 'Enables digging.', 'Diggable mission surfaces and buried rewards.'],
  ['rouge-treasure-scope', 'Rouge', 'Treasure Scope', 26, 'RougeTreasureScope', 'rougemdl', [205], 'Treasure Scope', 'Reveals otherwise hidden objects.', 'Hidden-object perception challenge.'],
  ['rouge-iron-boots', 'Rouge', 'Iron Boots', 27, 'RougeIronBoots', 'rougemdl', [201,203], 'Iron Boots', 'Kicks break iron containers.', 'Obstacle-specific reinforced kick.'],
  ['rouge-mystic-melody', 'Rouge', 'Mystic Melody', 28, 'RougeMysticMelody', null, [], null, 'Activates shrine paths toward Lost Chao.', 'Shrine puzzle interaction.'],
];
const upgrades = definitions.map(([id, character, name, upgradeBit, enumName, archive, ids, pickup, nativeEffect, proposal]) => {
  const field = pickup ? fieldPickups.get(`${pickup} Field Upgrade`) : null;
  if (pickup && !field) throw new Error(`Unmapped field pickup: ${pickup}`);
  const wearableModels = ids.map(nativeId => { const model = models[archive].get(nativeId); if (!model) throw new Error(`Unmapped wearable: ${archive}/${nativeId}`); return model; });
  const selected = ['shadow-air-shoes', 'knuckles-hammer-gloves'].includes(id);
  return { id, character, name, nativeEffect, upgradeBit, upgradeMaskHex: `0x${(2 ** upgradeBit).toString(16).padStart(8, '0')}`, nativeEnum: `UpgradeBits_${enumName}`,
    nativeSources: { wearableModels, fieldPickup: field ? { binary: 'sonic2app.exe', addressHex: `0x${field.address}`, pointerHex: `0x${field.pointer}`, modelPath: field.filename, texturePack: field.texture, evidence: `PC_SA2/CommonObjects.ini [${pickup} Field Upgrade]` } : null },
    mmo: { nativeEffectImplemented: false, proposal, visualAssetStatus: selected ? 'prepared-locally' : 'catalogued', ownerStarterSelection: selected, slot: id === 'shadow-air-shoes' ? 'footwear' : id === 'knuckles-hammer-gloves' ? 'hands' : null,
      visualAssets: selected ? [`${id}-left.glb`, `${id}-right.glb`] : [] },
    evidence: ['X-Hax sa2-mod-loader SA2Enums.h UpgradeBits/Upgrades', 'Licensed Prima Sonic Adventure2 guide: character upgrade descriptions; browser rules require separate implementation.'],
  };
});
if (upgrades.length !== 28 || new Set(upgrades.map(item => item.id)).size !== 28 || new Set(upgrades.map(item => item.upgradeBit)).size !== 28) throw new Error('Expected28 distinct native collectible upgrades.');
const catalog = { version: 1, status: 'Native character collectibles indexed; visual import and MMO ability implementation are separate.',
  sourceReferences: { nativeFlags: 'https://github.com/X-Hax/sa2-mod-loader/blob/master/SA2ModLoader/include/SA2Enums.h', modelStructures: 'https://github.com/X-Hax/SA2BModdingGuide/wiki/Character-Model-Structures', retailDescriptions: 'https://info.sonicretro.org/images/e/ee/SonicAdventure2_Prima_digital_guide.pdf', localMappings: 'Official X-Hax SA Tools release786 GameConfig/PC_SA2' },
  upgrades,
  specialStates: [{ id: 'super-sonic-native-state', character: 'Super Sonic', upgradeBit: 15, nativeEnum: 'UpgradeBits_SuperSonic', upgradeMaskHex: '0x00008000', collectible: false, note: 'Separate native super-state flag; not an ordinary field upgrade and not a Chaos Chao evolution.' }],
  alternateRoster: ['Amy', 'Metal Sonic', 'Tikal', 'Chaos0', 'Chao Walker', 'Dark Chao Walker'].map(character => ({ character, independentCollectibleUpgradeBits: [], note: 'No separate collectible upgrade bit appears for this character in the mapped native UpgradeBits enum. Base/alternate abilities require their own catalog and behavior audit.' })),
  assetNotes: { 'shadow-air-shoes': 'IDs78/79 are the native upgrade overlays; full exported jetboots also use original Shadow65 heel/toe mesh from native nodes13/14 and23/24. Each rigid assembled prop is recentered without stretching.', 'knuckles-hammer-gloves': 'IDs164/165 are the native right/left wearable metal gloves. Fit as rigid hand props; the Chao body and native skeleton remain intact.' },
};
await mkdir(path.join(root, 'shared'), { recursive: true });
await writeFile(path.join(root, 'shared/upgrade-catalog.json'), `${JSON.stringify(catalog, null, 2)}\n`);
console.log(JSON.stringify({ upgrades: upgrades.length, byCharacter: upgrades.reduce((counts, item) => { counts[item.character] = (counts[item.character] || 0) + 1; return counts; }, {}), wearableModels: upgrades.reduce((count, item) => count + item.nativeSources.wearableModels.length, 0), fieldPickups: upgrades.filter(item => item.nativeSources.fieldPickup).length, preparedVisualItems: upgrades.filter(item => item.mmo.visualAssetStatus === 'prepared-locally').map(item => item.id) }, null, 2));
