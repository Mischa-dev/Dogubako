// API types mirroring crates/core/src/spec.rs, engine.rs, crates/mcp/src/types.rs

export type CategoryId =
  | 'image' | 'video' | 'audio' | 'speech' | 'pdf' | 'files'
  | 'text' | 'dev' | 'data' | 'network' | 'design' | 'system';

export type PermissionId = 'host_fs_read' | 'host_fs_write' | 'network' | 'system_info' | 'process_info';

export interface EnumOption {
  value: string;
  label: string;
  /** Only usable while another param has one of these values. */
  only_if?: ShowIf | null;
  /** Why this option can't work on this install (set by the engine). */
  unavailable?: string | null;
}
export interface ShowIf { param: string; values: unknown[] }
/** A lower maximum while another param has one of the values. */
export interface MaxIf { param: string; values: unknown[]; max: number; unless_set?: string[] }

interface ParamBase {
  name: string;
  label: string;
  description?: string;
  required?: boolean;
  default?: unknown;
  advanced?: boolean;
  primary?: boolean;
  show_if?: ShowIf | null;
  /** Why this param can't be turned on here (set by the engine). */
  unavailable?: string | null;
}

export type ParamSpec = ParamBase & (
  | { type: 'text'; multiline?: boolean; max_len?: number; syntax?: string; placeholder?: string }
  | { type: 'secret' }
  | { type: 'integer'; min?: number; max?: number; max_if?: MaxIf[] }
  | { type: 'number'; min?: number; max?: number; step?: number; max_if?: MaxIf[] }
  | { type: 'boolean' }
  | { type: 'enum'; options: EnumOption[]; multi?: boolean }
  | { type: 'color' }
  | { type: 'file'; accept?: string[]; multiple?: boolean }
  | { type: 'path'; kind: 'file' | 'dir' | 'any' }
  | { type: 'json'; schema?: unknown }
);

export type ParamType = ParamSpec['type'];

export interface OutputSpec {
  kind: 'text' | 'data' | 'files';
  description?: string;
  text_mime?: string;
  data_schema?: unknown;
}

export interface MissingDep {
  id: string;
  name: string;
  hint: string;
  /** Engine id when the app can download this program itself. */
  downloadable?: string;
  download_bytes?: number;
}

export interface ModelOffer { id: string; name: string; size_bytes: number }

export interface Availability {
  available: boolean;
  platform_supported: boolean;
  missing?: MissingDep[];
  missing_model?: string;
  /** Required extension settings with no value yet. */
  unset_settings?: string[];
  /** With `missing_model`: the model group the tool needs and the model to offer. */
  model_group?: string;
  recommended_model?: ModelOffer;
  reason?: string;
}

export interface ToolInfo {
  id: string;
  name: string;
  description: string;
  category: CategoryId;
  keywords: string[];
  params: ParamSpec[];
  output: OutputSpec;
  platforms: string[];
  requires: string[];
  requires_model?: string;
  permissions: PermissionId[];
  timeout_secs: number;
  reports_progress: boolean;
  execution: 'native' | 'external' | 'model' | 'network';
  version: string;
  ui?: string;
  destructive: boolean;
  source: string;
  /** The tool checks its inputs before Run (`Backend.check`). */
  prechecks?: boolean;
  availability: Availability;
  permitted?: boolean;
  missing_permissions?: string[];
}

export interface Category { id: CategoryId; label: string; description: string; count: number }

export type JobStatus = 'queued' | 'running' | 'succeeded' | 'failed' | 'cancelled';

export interface Artifact {
  id?: string;
  name: string;
  mime: string;
  size: number;
  label?: string;
  url?: string;
  path?: string;
  /** Path with folders inside the result set ("docs/a.txt"); Save all and the zip keep it. */
  rel_path?: string;
}

export interface JobResult {
  text?: string;
  text_mime?: string;
  data: unknown;
  artifacts: Artifact[];
  warnings?: string[];
  duration_ms: number;
}

export interface ToolErrorView {
  code: string;
  message?: string;
  param?: string | null;
  dependency?: string;
  hint?: string;
  /** missing_model: the model the run needed, when the tool knows. */
  model?: string;
  program?: string;
  exit_code?: number | null;
  stderr?: string;
  seconds?: number;
  id?: string;
}

export interface JobView {
  id: string;
  tool_id: string;
  status: JobStatus;
  progress?: number;
  message?: string;
  result?: JobResult;
  error?: ToolErrorView;
  created_at: number;
  finished_at?: number;
  params?: Record<string, unknown>;
  owner?: string;
}

export interface FileView {
  id: string;
  name: string;
  mime: string;
  size: number;
  created_at: number;
  expires_at?: number;
  source: 'upload' | 'artifact' | string;
  job_id?: string;
  url?: string;
}

export interface Status {
  instance_name: string;
  version: string;
  auth: 'required' | 'none';
  setup_required: boolean;
  mcp_enabled: boolean;
  tools: number;
  mode: 'server' | 'desktop';
  max_upload_mb?: number;
  platform?: string;
  output_dir?: string;
  data_dir?: string;
}

export type Role = 'owner' | 'admin' | 'user' | 'agent';

export interface Policy {
  allow: string[];
  deny: string[];
  permissions: PermissionId[];
  max_concurrent_jobs?: number | null;
  quota_mb?: number | null;
  max_upload_mb?: number | null;
  mcp_discovery?: 'full' | 'compact' | null;
}

export interface Me {
  user: { id: string; username: string; role: Role; is_admin: boolean; via: string };
  csrf_token?: string;
  permissions: PermissionId[];
  policy?: Policy;
  token_scope?: Policy | null;
  storage?: { used: number; quota: number };
  max_upload?: number;
  /** Usernames of this instance's administrators. */
  admins?: string[];
}

export interface UserRow {
  id: string;
  username: string;
  role: Role;
  policy: Policy;
  disabled: boolean;
  created_at: number;
  last_seen_at?: number | null;
  description: string;
  kind: 'agent' | 'user';
  storage_used?: number;
}

export interface TokenRow {
  id: string;
  user_id: string;
  username: string;
  name: string;
  prefix: string;
  scope?: Policy | null;
  created_at: number;
  last_used_at?: number | null;
  expires_at?: number | null;
  revoked: boolean;
}

export interface NewToken { id: string; token: string; prefix: string; expires_at?: number | null }

export interface AuditRow {
  id: number; ts: number; actor: string; action: string; target: string; outcome: string; ip: string; detail: string;
}

export interface DepInfo {
  id: string;
  name: string;
  purpose?: string;
  found: boolean;
  version?: string | null;
  license?: string;
  install_hint?: string;
  install?: Partial<Record<'apt' | 'dnf' | 'pacman' | 'brew' | 'winget' | 'url', string | null>>;
  used_by?: string[];
  /** Desktop: `bundled` / `downloaded` for the app's own engines, else `system`. */
  source?: 'bundled' | 'downloaded' | 'system';
  /** Desktop: engine id when the app can download this program itself. */
  downloadable?: string;
  download_bytes?: number;
}

export interface ModelInfo {
  id: string;
  group: string;
  name: string;
  description?: string;
  size_bytes: number;
  license?: string;
  installed: boolean;
  verified?: boolean;
}

export interface ModelDownload { model: string; done: number; total: number; status: string; error?: string | null }

export interface AdminConfig {
  config: Record<string, unknown>;
  running_jobs: number;
  uptime_secs: number;
  permissions: { id: PermissionId; description: string }[];
}

/** An extension setting (Settings → Extensions). Secret values are never sent back. */
export type ExtensionSetting = ParamSpec & { secret: boolean; set: boolean; value?: unknown };

export interface ExtensionSettings { id: string; name: string; settings: ExtensionSetting[] }
