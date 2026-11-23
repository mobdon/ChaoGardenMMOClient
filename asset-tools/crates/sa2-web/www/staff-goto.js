// Staff go-to and follow: the server's `staff-goto` event {room, pose} as the
// line the game reads from /mmo/goto-in (sa2-boot chao_player.rs staff_goto):
// "seq area" or "seq area x y z". Area 0 is the lobby (the hub room).
const AREAS = { hub: 0, garden: 1, hero: 2, dark: 3, 'chao-race': 4, 'race-entrance': 5, kindergarten: 6, stadium: 9, 'chao-karate': 10 };
// null: the player is not in a Chao World room (a level, Tiny Chao Garden).
export function gotoLine(event, seq) {
  const area = Object.hasOwn(AREAS, event?.room) ? AREAS[event.room] : null;
  if (area === null) return null;
  const at = ['x', 'y', 'z'].map((k) => event.pose?.[k]);
  // A room alone when the player has no place there yet.
  return [seq, area, ...(at.every((v) => typeof v === 'number' && Number.isFinite(v)) ? at : [])].join(' ');
}

// The server keeps a Chao that is not aligned out of the Hero and Dark rooms
// (server game.rs chao-world), but the game can still be in that garden (an
// old save's door, ?area=). True once the room's snapshot still shows another
// zone 2 s after the page asked: the player goes back to the lobby.
export function gardenRefused(asked, serverZone, msSinceAsked) {
  return (asked === 'hero' || asked === 'dark') && serverZone !== asked && msSinceAsked > 2000;
}
