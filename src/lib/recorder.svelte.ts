// Reactive recorder state for one window. Each Tauri window runs its own copy and stays in sync through events.
import { errorMessage, onLevels, onRecorderError, onRecorderState, recorder, type Levels, type RecorderState } from "./api";

class RecorderStore {
  state = $state<RecorderState>({ status: "idle", startedAt: null, elapsedMs: 0, meetingId: null, sources: null });
  levels = $state<Levels>({ mic: 0, system: 0 });
  displayMs = $state(0);
  error = $state<string | null>(null);
  #base = 0;
  #at = 0;
  #timer: ReturnType<typeof setInterval> | undefined;
  #ready = false;

  async init() {
    if (this.#ready) return;
    this.#ready = true;
    await onRecorderState((s) => this.#apply(s));
    await onLevels((l) => (this.levels = l));
    await onRecorderError((message) => (this.error = message));
    await this.#run(recorder.state);
  }

  start = () => this.#run(recorder.start);
  stop = () => this.#run(recorder.stop);
  pause = () => this.#run(recorder.pause);
  resume = () => this.#run(recorder.resume);

  async #run(fn: () => Promise<RecorderState>) {
    try {
      this.error = null;
      this.#apply(await fn());
    } catch (err) {
      this.error = errorMessage(err);
    }
  }

  #apply(s: RecorderState) {
    this.state = s;
    this.#base = s.elapsedMs;
    this.#at = performance.now();
    this.displayMs = s.elapsedMs;
    clearInterval(this.#timer);
    if (s.status === "idle") this.levels = { mic: 0, system: 0 };
    if (s.status === "recording") {
      this.#timer = setInterval(() => {
        this.displayMs = this.#base + (performance.now() - this.#at);
      }, 250);
    }
  }
}

export const rec = new RecorderStore();
