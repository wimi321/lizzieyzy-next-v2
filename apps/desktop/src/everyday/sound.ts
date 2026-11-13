/**
 * 设置 → 播放落子声. The click of a stone on wood is made here: a short burst of filtered
 * noise over a low, quickly fading tone. Nothing is shipped or loaded, so there is no
 * sound file whose source or licence needs tracking.
 */
let audio: AudioContext | null = null;

function context(): AudioContext | null {
  if (audio) return audio;
  const Context = window.AudioContext ?? (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
  if (!Context) return null;
  audio = new Context();
  return audio;
}

export function playStone(): void {
  const output = context();
  if (!output) return;
  if (output.state === "suspended") void output.resume();
  const now = output.currentTime;

  // The hard tap: about 60 ms of noise, mostly gone after 10 ms.
  const length = Math.floor(output.sampleRate * 0.06);
  const buffer = output.createBuffer(1, length, output.sampleRate);
  const samples = buffer.getChannelData(0);
  const fade = output.sampleRate * 0.008;
  for (let index = 0; index < length; index += 1) samples[index] = (Math.random() * 2 - 1) * Math.exp(-index / fade);
  const tap = output.createBufferSource();
  tap.buffer = buffer;
  const band = output.createBiquadFilter();
  band.type = "bandpass";
  band.frequency.value = 2400;
  band.Q.value = 1.1;
  const tapGain = output.createGain();
  tapGain.gain.value = 0.55;
  tap.connect(band).connect(tapGain).connect(output.destination);

  // The board's body: a low tone that dies away in a tenth of a second.
  const body = output.createOscillator();
  body.type = "sine";
  body.frequency.setValueAtTime(190, now);
  body.frequency.exponentialRampToValueAtTime(120, now + 0.09);
  const bodyGain = output.createGain();
  bodyGain.gain.setValueAtTime(0.35, now);
  bodyGain.gain.exponentialRampToValueAtTime(0.001, now + 0.1);
  body.connect(bodyGain).connect(output.destination);

  tap.start(now);
  body.start(now);
  body.stop(now + 0.11);
}
