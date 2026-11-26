// Live transcription queue status, shared by pages in the main window.
import { onTranscriptionStatus, transcriptionStatus, type TranscriptionStatus } from "./api";

class TranscriptionStore {
  status = $state<TranscriptionStatus>({ state: "idle", meetingId: null, progress: 0, model: null });
  #ready = false;

  async init() {
    if (this.#ready) return;
    this.#ready = true;
    await onTranscriptionStatus((s) => (this.status = s));
    const s = await transcriptionStatus().catch(() => null);
    if (s) this.status = s;
  }

  progressFor(meetingId: number): number | null {
    return this.status.state === "working" && this.status.meetingId === meetingId ? this.status.progress : null;
  }
}

export const transcription = new TranscriptionStore();
