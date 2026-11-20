export function createGameAudio(button, canvas, { volumeInput, volumeOutput, voiceButton } = {}) {
  const read = key => { try { return localStorage.getItem(key); } catch { return null; } };
  const remember = (key, value) => { try { localStorage.setItem(key, value); } catch { /* Playback still works without storage. */ } };
  const legacyEnabled = read('chao-garden-audio') === 'on';
  let enabled = (read('chao-garden-music') ?? (legacyEnabled ? 'on' : 'off')) === 'on';
  let voicesEnabled = (read('chao-garden-voices') ?? (legacyEnabled ? 'on' : 'off')) === 'on';
  const savedVolume = read('chao-garden-music-volume');
  const parsedVolume = savedVolume === null || savedVolume.trim() === '' ? 22 : Number(savedVolume);
  let volume = Number.isFinite(parsedVolume) ? Math.max(0, Math.min(100, parsedVolume)) : 22;
  const music = document.createElement('audio');
  music.id = 'garden-music';
  music.loop = true;
  music.preload = 'none';
  music.volume = volume / 100;
  music.setAttribute('aria-hidden', 'true');
  const voice = document.createElement('audio');
  voice.id = 'npc-voice';
  voice.preload = 'none';
  voice.volume = 0.75;
  voice.setAttribute('aria-hidden', 'true');
  document.body.append(music, voice);
  let unlocked = false;
  let disposed = false;
  let zone = 'hub';
  let currentTrack = '';
  let lastVoiceAt = 0;
  const tracks = { hub: 'music-hub', garden: 'music-garden', hero: 'music-hero', dark: 'music-dark' };

  function updateButton() {
    button.textContent = enabled ? '♫ Music on' : '♪ Music off';
    button.setAttribute('aria-pressed', String(enabled));
    button.setAttribute('aria-label', enabled ? 'Mute music' : 'Enable music');
    if (voiceButton) {
      voiceButton.textContent = voicesEnabled ? 'Voices on' : 'Voices off';
      voiceButton.setAttribute('aria-pressed', String(voicesEnabled));
      voiceButton.setAttribute('aria-label', voicesEnabled ? 'Mute NPC voices' : 'Enable NPC voices');
    }
    if (volumeInput) {
      volumeInput.value = String(volume);
      volumeInput.style.setProperty('--music-level', `${volume}%`);
      volumeInput.setAttribute('aria-valuetext', `${Math.round(volume)} percent`);
    }
    if (volumeOutput) volumeOutput.textContent = `${Math.round(volume)}%`;
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
    music.play().catch(error => {
      if (disposed || !enabled) return;
      if (error?.name === 'NotAllowedError') { unlocked = false; canvas.dataset.musicState = 'waiting-for-interaction'; }
      else if (error?.name !== 'AbortError') canvas.dataset.musicState = 'unavailable';
    });
  }
  function unlock() {
    unlocked = true;
    playMusic();
  }
  function toggleMusic() {
    enabled = !enabled;
    unlocked = true;
    remember('chao-garden-music', enabled ? 'on' : 'off');
    updateButton();
    playMusic();
  }
  function toggleVoices() {
    voicesEnabled = !voicesEnabled; unlocked = true;
    remember('chao-garden-voices', voicesEnabled ? 'on' : 'off');
    if (!voicesEnabled) voice.pause();
    updateButton();
    playMusic();
  }
  function changeVolume() {
    const next = Number(volumeInput.value);
    if (!Number.isFinite(next)) return;
    volume = Math.max(0, Math.min(100, next)); music.volume = volume / 100;
    remember('chao-garden-music-volume', String(volume));
    updateButton();
  }
  // Keep native slider/button keyboard behavior from reaching the game inputs.
  const controls = button.closest('.audio-controls') || button;
  const controlKeys = event => event.stopPropagation();
  controls.addEventListener('keydown', controlKeys);
  button.addEventListener('click', toggleMusic);
  voiceButton?.addEventListener('click', toggleVoices);
  volumeInput?.addEventListener('input', changeVolume);
  music.addEventListener('playing', () => { canvas.dataset.musicState = 'playing'; });
  music.addEventListener('pause', () => { canvas.dataset.musicState = 'paused'; });
  music.addEventListener('error', () => { canvas.dataset.musicState = 'unavailable'; });
  const onGesture = () => { if (!unlocked) unlock(); };
  window.addEventListener('pointerdown', onGesture);
  window.addEventListener('keydown', onGesture);
  updateButton();
  return {
    setZone(next) { if (zone !== next) { zone = next; playMusic(); } },
    speak(character) {
      if (!voicesEnabled || !unlocked || !['knuckles', 'tails', 'amy'].includes(character) || Date.now() - lastVoiceAt < 1500) return;
      lastVoiceAt = Date.now();
      voice.pause();
      voice.src = `/api/assets/voice-${character}.mp3`;
      canvas.dataset.npcVoice = character;
      voice.play().catch(() => {});
    },
    dispose() {
      disposed = true;
      controls.removeEventListener('keydown', controlKeys);
      button.removeEventListener('click', toggleMusic);
      voiceButton?.removeEventListener('click', toggleVoices);
      volumeInput?.removeEventListener('input', changeVolume);
      music.pause(); voice.pause(); music.remove(); voice.remove();
      window.removeEventListener('pointerdown', onGesture);
      window.removeEventListener('keydown', onGesture);
    },
  };
}
