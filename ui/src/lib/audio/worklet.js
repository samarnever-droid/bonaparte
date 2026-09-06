// Device-side streaming only. All media decoding, gain/pan automation and mixing
// occur in Rust. This processor owns the sample clock and bounded PCM queue.
class BonaparteAudio extends AudioWorkletProcessor {
  constructor() {
    super();
    this.generation = 0;
    this.blocks = [];
    this.cursor = 0;
    this.end = 0;
    this.playing = false;
    this.loop = false;
    this.mode = "play";
    this.ticks = 0;
    this.underflows = 0;
    this.volume = 1;
    this.targetVolume = 1;
    this.origin = 0;
    this.loops = 0;
    this.crossfade = null;
    this.port.onmessage = ({ data: m }) => {
      if (m.type === "swap") {
        this.crossfade = this.playing
          ? {
              blocks: this.blocks,
              end: this.end,
              cursor: this.cursor,
              remaining: Math.ceil(sampleRate * 0.005),
              total: Math.ceil(sampleRate * 0.005),
            }
          : null;
        this.generation = m.generation;
        this.blocks = m.blocks.map((b) => ({ ...b, end: b.start + b.pcm.length / 2 }));
        this.end = m.end;
        this.loop = m.loop;
        this.cursor = Math.min(this.cursor, this.end - 1);
        this.playing = true;
      }
      if (m.type === "reset") {
        this.crossfade = null;
        this.generation = m.generation;
        this.blocks = [];
        this.playing = false;
        this.cursor = m.start;
        this.end = m.end;
        this.loop = m.loop;
        this.mode = m.mode;
        this.underflows = 0;
        this.origin = m.start;
        this.loops = 0;
      }
      if (m.type === "chunk" && m.generation === this.generation) {
        this.blocks = this.blocks.filter((b) => b.start !== m.start);
        this.blocks.push({ start: m.start, pcm: m.pcm, end: m.start + m.pcm.length / 2 });
        if (this.blocks.length > 16) {
          const distance = (b) =>
            this.cursor >= b.start && this.cursor < b.end
              ? -1
              : (b.start - this.cursor + this.end) % Math.max(1, this.end);
          this.blocks.sort((a, b) => distance(a) - distance(b));
          this.blocks.length = 16;
        }
      }
      if (m.type === "start" && m.generation === this.generation) this.playing = true;
      if (m.type === "stop") {
        this.playing = false;
        this.blocks = [];
        this.crossfade = null;
      }
      if (m.type === "monitor") this.targetVolume = Math.max(0, Math.min(1, m.value));
    };
  }
  process(inputs, outputs) {
    const out = outputs[0];
    if (!out || out.length < 2) return true;
    let peak = [0, 0],
      sum = [0, 0],
      buffering = false;
    for (let i = 0; i < out[0].length; i++) {
      if (!this.playing) {
        out[0][i] = out[1][i] = 0;
        continue;
      }
      if (this.cursor >= this.end) {
        if (this.loop) {
          this.cursor = 0;
          this.loops++;
        } else {
          this.playing = false;
          this.port.postMessage({ type: "ended", generation: this.generation, mode: this.mode });
          out[0][i] = out[1][i] = 0;
          continue;
        }
      }
      const block = this.blocks.find((b) => this.cursor >= b.start && this.cursor < b.end);
      if (!block) {
        out[0][i] = out[1][i] = 0;
        buffering = true;
        this.underflows++;
        continue;
      }
      const offset = (this.cursor - block.start) * 2;
      this.volume += (this.targetVolume - this.volume) * Math.min(1, 1 / (sampleRate * 0.005));
      const auditionGain =
        this.mode === "scrub"
          ? Math.max(
              0,
              Math.min(
                1,
                (this.cursor - this.origin) / (sampleRate * 0.005),
                (this.end - this.cursor) / (sampleRate * 0.012),
              ),
            )
          : 1;
      const transition = this.crossfade;
      const old = transition?.blocks.find(
        (b) => transition.cursor >= b.start && transition.cursor < b.end,
      );
      for (let c = 0; c < 2; c++) {
        let raw = block.pcm[offset + c] * auditionGain;
        if (transition && old) {
          const alpha = 1 - transition.remaining / transition.total;
          raw = old.pcm[(transition.cursor - old.start) * 2 + c] * (1 - alpha) + raw * alpha;
        }
        out[c][i] = raw * this.volume;
        peak[c] = Math.max(peak[c], Math.abs(raw));
        sum[c] += raw * raw;
      }
      if (transition) {
        transition.cursor = (transition.cursor + 1) % transition.end;
        transition.remaining--;
        if (transition.remaining <= 0) this.crossfade = null;
      }
      this.cursor++;
    }
    this.ticks += out[0].length;
    if (this.ticks >= 512) {
      this.ticks = 0;
      this.port.postMessage({
        type: "clock",
        generation: this.generation,
        cursor: this.cursor,
        origin: this.origin,
        loops: this.loops,
        contextFrame: currentFrame + out[0].length,
        playing: this.playing,
        mode: this.mode,
        buffering,
        underflows: this.underflows,
        peak,
        rms: sum.map((v) => Math.sqrt(v / out[0].length)),
      });
    }
    return true;
  }
}
registerProcessor("bonaparte-audio", BonaparteAudio);
