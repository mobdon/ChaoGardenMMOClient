export function createGameAudio(button, canvas) {
  const music = document.createElement('audio');
  music.id = 'garden-music';
  music.loop = true;
  music.preload = 'none';
  music.volume = 0.22;
  music.setAttribute('aria-hidden', 'true');
  const voice = document.createElement('audio');
  voice.id = 'npc-voice';
  voice.preload = 'none';
  voice.volume = 0.75;
  voice.setAttribute('aria-hidden', 'true');
  document.body.append(music, voice);
  let enabled = localStorage.getItem('chao-garden-audio') === 'on';
  let unlocked = false;
  let zone = 'hub';
  let currentTrack = '';
  let lastVoiceAt = 0;
  const tracks = { hub: 'music-hub', garden: 'music-garden', hero: 'music-hero', dark: 'music-dark' };

  function updateButton() {
    button.textContent = enabled ? '♫ Sound on' : '♪ Sound off';
    button.setAttribute('aria-pressed', String(enabled));
    button.setAttribute('aria-label', enabled ? 'Mute music and NPC voices' : 'Enable music and NPC voices');
  }
  function playMusic() {
    const track = tracks[zone];
    if (!enabled || !unlocked || !track) { music.pause(); return; }
    if (track !== currentTrack) {
      music.pause();
      music.src = `/api/assets/${track}.mp3`;
      currentTrack = track;
      canvas.dataset.musicTrack = track;
    }
    music.play().catch(() => { canvas.dataset.musicState = 'waiting-for-interaction'; });
  }
  function unlock() {
    unlocked = true;
    playMusic();
  }
  button.addEventListener('click', () => {
    enabled = !enabled;
    unlocked = true;
    localStorage.setItem('chao-garden-audio', enabled ? 'on' : 'off');
    if (!enabled) voice.pause();
    updateButton();
    playMusic();
  });
  music.addEventListener('playing', () => { canvas.dataset.musicState = 'playing'; });
  music.addEventListener('pause', () => { canvas.dataset.musicState = 'paused'; });
  music.addEventListener('error', () => { canvas.dataset.musicState = 'unavailable'; });
  const onGesture = () => { if (enabled && !unlocked) unlock(); };
  window.addEventListener('pointerdown', onGesture);
  window.addEventListener('keydown', onGesture);
  updateButton();
  return {
    setZone(next) { if (zone !== next) { zone = next; playMusic(); } },
    speak(character) {
      if (!enabled || !unlocked || !['knuckles', 'tails', 'amy'].includes(character) || Date.now() - lastVoiceAt < 1500) return;
      lastVoiceAt = Date.now();
      voice.pause();
      voice.src = `/api/assets/voice-${character}.mp3`;
      canvas.dataset.npcVoice = character;
      voice.play().catch(() => {});
    },
    dispose() {
      music.pause(); voice.pause(); music.remove(); voice.remove();
      window.removeEventListener('pointerdown', onGesture);
      window.removeEventListener('keydown', onGesture);
    },
  };
}
