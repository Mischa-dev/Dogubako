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
    case 'external_failure': {
      const stderr = (e.stderr ?? '').trim();
      return {
        title: `${e.program ?? 'External program'} failed`,
        message: e.exit_code != null ? `It exited with code ${e.exit_code}.` : 'It stopped unexpectedly.',
        details: stderr || undefined,
      };
    }
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

export function errorText(e: unknown): string {
  if (!e) return '';
  if (typeof e === 'string') return e;
  if (e instanceof Error) return e.message;
  const anyE = e as ToolErrorView;
  return describeError(anyE).message;
}
