// Звуковые подсказки на WebAudio — без файлов-ассетов
let ctx: AudioContext | null = null;

function beep(
  freq: number,
  dur: number,
  delay: number,
  type: OscillatorType = "sine",
  vol = 0.12,
) {
  try {
    ctx = ctx ?? new AudioContext();
    const t0 = ctx.currentTime + delay;
    const osc = ctx.createOscillator();
    const gain = ctx.createGain();
    osc.type = type;
    osc.frequency.value = freq;
    gain.gain.setValueAtTime(vol, t0);
    gain.gain.exponentialRampToValueAtTime(0.0001, t0 + dur);
    osc.connect(gain);
    gain.connect(ctx.destination);
    osc.start(t0);
    osc.stop(t0 + dur);
  } catch {
    // звук не критичен — молча пропускаем
  }
}

export const sounds = {
  success: () => {
    beep(660, 0.12, 0);
    beep(880, 0.16, 0.12);
  },
  error: () => {
    beep(220, 0.2, 0, "square", 0.08);
    beep(160, 0.25, 0.18, "square", 0.08);
  },
  notify: () => beep(520, 0.1, 0),
};
