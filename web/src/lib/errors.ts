import type { ToolErrorView } from './types';

export interface FriendlyError {
  title: string;
  message: string;
  hint?: string;
  details?: string;
  param?: string;
}

export function describeError(e: ToolErrorView | null | undefined): FriendlyError {
  if (!e) return { title: 'Something went wrong', message: 'The tool failed without an error message.' };
  const msg = e.message?.trim();
  switch (e.code) {
    case 'invalid_input':
      return { title: 'Check your input', message: msg || 'One of the inputs is not valid.', param: e.param ?? undefined };
    case 'missing_dependency':
      return {
        title: `Needs ${e.dependency ?? 'an external program'}`,
        message: `This tool uses ${e.dependency ?? 'a program'} which isn't installed on ${'this machine'}.`,
        hint: e.hint || msg,
      };
    case 'missing_model':
      return { title: 'Model not installed', message: msg || 'Download the required model in Settings → Models.', hint: 'Open Settings → Models to install it.' };
    case 'unsupported':
      return { title: 'Not supported here', message: msg || 'This operation is not supported on this platform.' };
    case 'permission_denied':
    case 'forbidden':
      return { title: 'Not allowed', message: msg || 'Your account is not permitted to do this.', hint: 'Ask an administrator to grant access.' };
    case 'cancelled':
      return { title: 'Cancelled', message: 'The run was cancelled.' };
    case 'timeout':
      return { title: 'Timed out', message: e.seconds ? `The tool did not finish within ${e.seconds} seconds.` : msg || 'The tool took too long.', hint: 'Try a smaller input or simpler settings.' };
    case 'limit_exceeded':
    case 'too_large':
      return { title: 'Limit exceeded', message: msg || 'The input is larger than this instance allows.' };
    case 'external':
    case 'external_failure': {
      const stderr = (e.stderr ?? '').trim();
      const reason = stderrReason(stderr) || (msg && msg !== 'Error: external' ? msg : '');
      const program = e.program || 'External program';
      return {
        title: e.exit_code != null ? `${program} failed (exit code ${e.exit_code})` : `${program} failed`,
        message: reason || (e.exit_code != null ? 'It exited without printing a reason.' : 'It stopped unexpectedly.'),
        details: stderr || undefined,
      };
    }
    case 'not_installed':
      return { title: 'Tool not installed', message: msg || "This tool's pack is turned off. Turn it on to use it." };
    case 'not_found':
      return { title: 'Not found', message: msg || 'The requested item does not exist.' };
    case 'unauthorized':
      return { title: 'Signed out', message: 'Your session has ended. Sign in again.' };
    case 'rate_limited':
      return { title: 'Slow down', message: msg || 'Too many requests; try again shortly.' };
    case 'network':
      return { title: 'Connection problem', message: msg || 'Could not reach the server.' };
    case 'conflict':
      return { title: 'Conflict', message: msg || 'That conflicts with the current state.' };
    default:
      return { title: 'Something went wrong', message: msg || `Error: ${e.code}` };
  }
}

const GENERIC_TRAILERS = /^(conversion failed!?|exiting\.?|aborted\.?)$/i;
const FAILURE_WORDS = /\b(error|errors|invalid|failed|failure|fatal|cannot|can't|could not|couldn't|unable|unsupported|not supported|not found|no such|denied|incorrect|unknown|unrecognized|missing|not an?|bad|no (?:decode|encode) delegate|too (?:large|long|big|many)|out of memory|panicked|segmentation fault)\b/i;
const REASON_MAX = 300;

/** The most telling line of a program's stderr, for a one-line summary. */
export function stderrReason(stderr: string | undefined | null): string {
  const lines = (stderr ?? '')
    .split(/\r?\n/)
    .map((l) => l.trim().replace(/^\[([\w.-]+) @ 0x[0-9a-f]+\]\s*/i, '$1: ').replace(/\s+@ (?:error|warning)\/\S+$/, ''))
    .filter((l) => l && !GENERIC_TRAILERS.test(l));
  if (!lines.length) return '';
  const line = lines.find((l) => FAILURE_WORDS.test(l)) ?? lines[lines.length - 1];
  return line.length > REASON_MAX ? `${line.slice(0, REASON_MAX - 1)}…` : line;
}

export function errorText(e: unknown): string {
  if (!e) return '';
  if (typeof e === 'string') return e;
  if (e instanceof Error) return e.message;
  const anyE = e as ToolErrorView;
  return describeError(anyE).message;
}
