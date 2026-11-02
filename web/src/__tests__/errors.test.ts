import { describe, expect, it } from 'vitest';
import { describeError } from '../lib/errors';

describe('describeError', () => {
  it('maps missing_dependency to a hint', () => {
    const f = describeError({ code: 'missing_dependency', dependency: 'FFmpeg', hint: 'sudo apt install ffmpeg' });
    expect(f.title).toBe('Needs FFmpeg');
    expect(f.hint).toBe('sudo apt install ffmpeg');
  });
  it('maps invalid_input with its param', () => {
    expect(describeError({ code: 'invalid_input', param: 'quality', message: 'too high' })).toMatchObject({ title: 'Check your input', param: 'quality', message: 'too high' });
  });
  it('exposes stderr for external failures', () => {
    const f = describeError({ code: 'external_failure', program: 'ffmpeg', exit_code: 1, stderr: 'boom\n' });
    expect(f.title).toBe('ffmpeg failed');
    expect(f.message).toMatch(/code 1/);
    expect(f.details).toBe('boom');
  });
  it('handles timeouts, permissions and unknown codes', () => {
    expect(describeError({ code: 'timeout', seconds: 30 }).message).toMatch(/30 seconds/);
    expect(describeError({ code: 'permission_denied', message: 'nope' }).title).toBe('Not allowed');
    expect(describeError({ code: 'weird' }).message).toBe('Error: weird');
    expect(describeError(undefined).title).toBe('Something went wrong');
  });
});
