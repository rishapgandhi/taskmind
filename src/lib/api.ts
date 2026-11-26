// Typed wrappers around Tauri commands and events. All UI code calls the backend through here.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type RecorderStatus = "idle" | "recording" | "paused";

export interface SourceStatus {
  /** Device name when recording from it. */
  mic: string | null;
  system: string | null;
  warnings: string[];
}

export interface RecorderState {
  status: RecorderStatus;
  startedAt: string | null;
  elapsedMs: number;
  meetingId: number | null;
  sources: SourceStatus | null;
}

export interface Levels {
  mic: number;
  system: number;
}

export type MeetingStatus = "recording" | "finalizing" | "queued" | "transcribing" | "done" | "failed";

export interface Meeting {
  id: number;
  title: string | null;
  startedAt: string;
  endedAt: string | null;
  durationS: number | null;
  audioPath: string | null;
  audioBytes: number | null;
  status: MeetingStatus;
  language: string | null;
  error: string | null;
  hasNotes: boolean;
  notesError: string | null;
}

export type Speaker = "you" | "others" | "unknown" | string;

export interface Segment {
  startMs: number;
  endMs: number;
  speaker: Speaker;
  text: string;
}

export interface MeetingDetail {
  meeting: Meeting;
  segments: Segment[];
}

export interface SearchHit {
  meetingId: number;
  title: string | null;
  startedAt: string;
  startMs: number;
  speaker: Speaker;
  snippet: string;
}

export interface ModelStatus {
  id: string;
  label: string;
  description: string;
  sizeBytes: number;
  downloaded: boolean;
  recommended: boolean;
}

export interface ModelProgress {
  id: string;
  downloaded: number;
  total: number;
  done: boolean;
  error: string | null;
}

export interface TranscriptionStatus {
  state: "idle" | "working" | "paused" | "needs_model";
  meetingId: number | null;
  progress: number;
  model: string | null;
}

export interface TrackerStatus {
  enabled: boolean;
  idle: boolean;
  problem: string | null;
  appName: string | null;
  windowTitle: string | null;
  projectId: number | null;
}

export interface Total {
  key: string;
  projectId: number | null;
  seconds: number;
}

export interface Block {
  kind: "work" | "meeting";
  projectId: number | null;
  meetingId: number | null;
  start: string;
  end: string;
  seconds: number;
  details: string[];
}

export interface DayReport {
  date: string;
  workSeconds: number;
  meetingSeconds: number;
  idleSeconds: number;
  projects: Total[];
  apps: Total[];
  blocks: Block[];
}

export interface DaySummary {
  date: string;
  workSeconds: number;
  meetingSeconds: number;
  projects: Total[];
}

export type RuleKind = "window_title" | "app_name" | "url" | "git_branch" | "directory";

export interface Rule {
  kind: RuleKind;
  pattern: string;
  priority: number;
}

export interface Project {
  id: number;
  name: string;
  color: string | null;
  rules: Rule[];
}

export interface TimesheetEntry {
  projectId: number | null;
  projectName: string;
  start: string;
  end: string;
  minutes: number;
  description: string;
}

export interface Timesheet {
  date: string;
  status: "generated" | "draft" | "approved";
  entries: TimesheetEntry[];
}

export interface ActionItem {
  text: string;
  owner: string | null;
  due: string | null;
  done: boolean;
}

export interface Notes {
  summary: string;
  decisions: string[];
  actionItems: ActionItem[];
}

export interface StoredNotes {
  notes: Notes;
  provider: string | null;
  model: string | null;
  generatedAt: string | null;
  edited: boolean;
}

export interface DayActionItem {
  meetingId: number;
  meetingTitle: string | null;
  index: number;
  item: ActionItem;
}

export interface AppInfo {
  version: string;
  dbPath: string | null;
  schemaVersion: number;
  meetingCount: number;
  activityCount: number;
  shortcuts: { toggleRecording: string; showApp: string };
}

export interface CommandError {
  code: string;
  message: string;
}

export const SETTINGS = {
  onboardingDone: "onboarding.done",
  recordingsDir: "storage.recordingsDir",
  deleteAudioAfterDays: "storage.deleteAudioAfterDays",
  notesProvider: "notes.provider",
  notesModel: "notes.model",
  notesBaseUrl: "notes.baseUrl",
  notesAuto: "notes.auto",
  trackingEnabled: "tracking.enabled",
  trackingIdleMinutes: "tracking.idleMinutes",
  trackingBlocklist: "tracking.blocklist",
  trackingBrowserUrls: "tracking.browserUrls",
  meetingDetection: "meetings.detection",
  meetingKeywords: "meetings.keywords",
  consentReminder: "recording.consentReminder",
  transcribeModel: "transcription.model",
  transcribeLanguage: "transcription.language",
  transcribePaused: "transcription.paused",
} as const;

export const recorder = {
  state: () => invoke<RecorderState>("recorder_state"),
  start: () => invoke<RecorderState>("recorder_start"),
  stop: () => invoke<RecorderState>("recorder_stop"),
  pause: () => invoke<RecorderState>("recorder_pause"),
  resume: () => invoke<RecorderState>("recorder_resume"),
};

export const listMeetings = (opts: { limit?: number; date?: string } = {}) =>
  invoke<Meeting[]>("list_meetings", { limit: opts.limit, dateStr: opts.date });
export const revealMeeting = (id: number) => invoke<void>("reveal_meeting", { id });
export const getMeeting = (id: number) => invoke<MeetingDetail>("get_meeting", { id });
export const renameMeeting = (id: number, title: string) => invoke<void>("rename_meeting", { id, title });
export const deleteMeeting = (id: number) => invoke<void>("delete_meeting", { id });
export const retranscribeMeeting = (id: number) => invoke<void>("retranscribe_meeting", { id });
export const searchTranscripts = (query: string) => invoke<SearchHit[]>("search_transcripts", { query });

export const listModels = () => invoke<ModelStatus[]>("list_models");
export const downloadModel = (id: string) => invoke<void>("download_model", { id });
export const deleteModel = (id: string) => invoke<void>("delete_model", { id });
export const transcriptionStatus = () => invoke<TranscriptionStatus | null>("transcription_status");

export const trackingStatus = () => invoke<TrackerStatus>("tracking_status");
export const requestTrackingPermission = () => invoke<void>("request_tracking_permission");
export const dayReport = (date: string) => invoke<DayReport>("day_report", { dateStr: date });
export const rangeReport = (from: string, to: string) => invoke<DaySummary[]>("range_report", { from, to });
export const deleteDayActivity = (date: string) => invoke<number>("delete_day_activity", { dateStr: date });
export const listProjects = () => invoke<Project[]>("list_projects");
export const saveProject = (project: { id: number | null; name: string; color: string | null; rules: Rule[] }) =>
  invoke<number>("save_project", { project });
export const deleteProject = (id: number) => invoke<void>("delete_project", { id });
export const getTimesheet = (date: string) => invoke<Timesheet>("get_timesheet", { dateStr: date });
export const saveTimesheet = (date: string, entries: TimesheetEntry[], approved: boolean) =>
  invoke<void>("save_timesheet", { dateStr: date, entries, approved });
export const resetTimesheet = (date: string) => invoke<void>("reset_timesheet", { dateStr: date });
export const exportTimesheet = (from: string, to: string, format: "csv" | "json") =>
  invoke<string>("export_timesheet", { from, to, format });

export const getNotes = (id: number) => invoke<StoredNotes | null>("get_notes", { id });
export const generateNotes = (id: number) => invoke<Notes>("generate_notes", { id });
export const saveNotes = (id: number, notes: Notes) => invoke<void>("save_notes", { id, notes });
export const setNotesKey = (provider: string, key: string) => invoke<void>("set_notes_key", { provider, key });
export const hasNotesKey = (provider: string) => invoke<boolean>("has_notes_key", { provider });
export const checkNotesProvider = () => invoke<string>("check_notes_provider");
export const dayActionItems = (date: string) => invoke<DayActionItem[]>("day_action_items", { dateStr: date });

export const backupNow = () => invoke<string>("backup_now");
export const restoreBackup = (path: string) => invoke<void>("restore_backup", { path });
export const importV1Status = () => invoke<{ available: boolean; doneAt: string | null }>("import_v1_status");
export const runImportV1 = () =>
  invoke<{ found: boolean; projects: number; activities: number; meetings: number }>("run_import_v1");
export const recordingsFolder = () => invoke<string>("recordings_folder");

export const showMainWindow = () => invoke<void>("show_main_window");
export const appInfo = () => invoke<AppInfo>("app_info");
export const getSetting = <T>(key: string) => invoke<T | null>("get_setting", { key });
export const setSetting = (key: string, value: unknown) => invoke<void>("set_setting", { key, value });

export const onRecorderState = (cb: (s: RecorderState) => void): Promise<UnlistenFn> =>
  listen<RecorderState>("recorder://state", (e) => cb(e.payload));

export const onLevels = (cb: (l: Levels) => void): Promise<UnlistenFn> =>
  listen<Levels>("recorder://level", (e) => cb(e.payload));

export const onRecorderError = (cb: (message: string) => void): Promise<UnlistenFn> =>
  listen<string>("recorder://error", (e) => cb(e.payload));

export const onMeetingsChanged = (cb: () => void): Promise<UnlistenFn> =>
  listen("meetings://changed", () => cb());

export const onTranscriptionStatus = (cb: (s: TranscriptionStatus) => void): Promise<UnlistenFn> =>
  listen<TranscriptionStatus>("transcribe://status", (e) => cb(e.payload));

export const onModelProgress = (cb: (p: ModelProgress) => void): Promise<UnlistenFn> =>
  listen<ModelProgress>("models://progress", (e) => cb(e.payload));

export const onTrackingStatus = (cb: (s: TrackerStatus) => void): Promise<UnlistenFn> =>
  listen<TrackerStatus>("tracking://status", (e) => cb(e.payload));

export const onMeetingDetected = (cb: (title: string) => void): Promise<UnlistenFn> =>
  listen<{ title: string }>("meeting://detected", (e) => cb(e.payload.title));

export const onNotesChanged = (cb: (e: { meetingId: number; state: string; error?: string }) => void): Promise<UnlistenFn> =>
  listen<{ meetingId: number; state: string; error?: string }>("notes://changed", (e) => cb(e.payload));

export const onConsentReminder = (cb: () => void): Promise<UnlistenFn> =>
  listen("recorder://consent-reminder", () => cb());

export function errorMessage(err: unknown): string {
  if (err && typeof err === "object" && "message" in err) return String((err as CommandError).message);
  return String(err);
}

export function formatBytes(bytes: number | null): string {
  if (bytes == null) return "–";
  if (bytes < 1024 * 1024) return `${Math.max(1, Math.round(bytes / 1024))} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function formatDuration(seconds: number | null): string {
  if (seconds == null) return "–";
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = seconds % 60;
  if (h > 0) return `${h} h ${m} min`;
  if (m > 0) return `${m} min ${s} s`;
  return `${s} s`;
}

export function formatClock(ms: number): string {
  const total = Math.floor(ms / 1000);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const pad = (n: number) => n.toString().padStart(2, "0");
  return h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${m}:${pad(s)}`;
}

export const SPEAKER_LABEL: Record<string, string> = { you: "You", others: "Others", unknown: "Unknown" };

/** Split an FTS snippet with [match] markers into parts, without using HTML. */
export function snippetParts(snippet: string): { text: string; hit: boolean }[] {
  const parts: { text: string; hit: boolean }[] = [];
  const re = /\[([^\]]*)\]/g;
  let last = 0;
  for (const m of snippet.matchAll(re)) {
    if (m.index! > last) parts.push({ text: snippet.slice(last, m.index), hit: false });
    parts.push({ text: m[1], hit: true });
    last = m.index! + m[0].length;
  }
  if (last < snippet.length) parts.push({ text: snippet.slice(last), hit: false });
  return parts;
}

/** Local date as YYYY-MM-DD. */
export function isoDate(d: Date): string {
  const pad = (n: number) => n.toString().padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

export function addDays(date: string, days: number): string {
  const [y, m, d] = date.split("-").map(Number);
  return isoDate(new Date(y, m - 1, d + days));
}

export function formatHours(seconds: number): string {
  const m = Math.round(seconds / 60);
  if (m < 60) return `${m}m`;
  return `${Math.floor(m / 60)}h ${(m % 60).toString().padStart(2, "0")}m`;
}

const PALETTE = ["#4f46e5", "#0d9488", "#d97706", "#db2777", "#2563eb", "#65a30d", "#9333ea", "#dc2626"];

export function projectColor(p: { id: number; color: string | null } | undefined | null): string {
  if (!p) return "#9a9aa6";
  return p.color ?? PALETTE[p.id % PALETTE.length];
}

export function formatElapsed(ms: number): string {
  const total = Math.floor(ms / 1000);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const pad = (n: number) => n.toString().padStart(2, "0");
  return `${pad(h)}:${pad(m)}:${pad(s)}`;
}
