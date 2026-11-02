import type {
  Artifact, Category, DepInfo, JobView, Me, ModelDownload, ModelInfo, Status, ToolErrorView, ToolInfo,
} from '../types';

/** A file chosen as tool input. `ref` is a server file id (`f_…`) or a local absolute path (desktop). */
export interface FileRef {
  ref: string;
  name: string;
  size: number;
  mime: string;
}

export interface RunHandle {
  /** Resolves with the terminal job (never rejects for tool errors; rejects only on transport failures). */
  done: Promise<JobView>;
  cancel(): Promise<void>;
}

export interface RunOptions {
  onUpdate?: (job: JobView) => void;
}

export interface PickOptions {
  multiple?: boolean;
  accept?: string[];
  directory?: boolean;
}

/** One interface over the HTTP API (server mode) and Tauri commands (desktop mode). */
export interface Backend {
  readonly mode: 'server' | 'desktop';
  getStatus(): Promise<Status>;
  getMe(): Promise<Me | null>;
  listTools(): Promise<ToolInfo[]>;
  listCategories(): Promise<Category[] | null>;
  run(toolId: string, params: Record<string, unknown>, opts?: RunOptions): RunHandle;

  /** Turn browser File objects into tool input references (upload or stage). */
  uploadFiles(files: File[], onProgress?: (fraction: number) => void, signal?: AbortSignal): Promise<FileRef[]>;
  /** Native picker (desktop only). Returns null when cancelled; undefined when unsupported. */
  pickFiles?(opts: PickOptions): Promise<FileRef[] | null>;
  pickPath?(kind: 'file' | 'dir' | 'any'): Promise<string | null>;
  /** Subscribe to OS-level drag & drop of file paths (desktop). Returns unsubscribe. */
  onNativeDrop?(cb: (refs: FileRef[]) => void): Promise<() => void>;
  describeFile(ref: string): Promise<FileRef>;
  /** URL usable in <img>/<video>/<iframe> for an input file reference. */
  fileUrl(ref: FileRef): string;
  artifactUrl(a: Artifact): string;
  downloadArtifact(a: Artifact): Promise<void>;
  revealArtifact?(a: Artifact): Promise<void>;
  zipUrl?(jobId: string): string;

  listJobs(limit?: number): Promise<JobView[]>;
  deleteJob(id: string): Promise<void>;

  deps(): Promise<{ dependencies: DepInfo[]; platform?: string }>;
  models(): Promise<{ models: ModelInfo[]; downloads: ModelDownload[] }>;
  modelDownload(id: string): Promise<void>;
  modelRemove(id: string): Promise<void>;
}

/** Thrown for non-2xx API responses. */
export class ApiError extends Error {
  status: number;
  code: string;
  detail?: ToolErrorView;
  constructor(status: number, code: string, message: string, detail?: ToolErrorView) {
    super(message);
    this.status = status;
    this.code = code;
    this.detail = detail;
  }
  /** Best structured tool error for display. */
  toToolError(): ToolErrorView {
    return this.detail ?? { code: this.code, message: this.message };
  }
}

export const isTauri = (): boolean =>
  typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
