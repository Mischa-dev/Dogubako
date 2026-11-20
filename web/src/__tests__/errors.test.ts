import { describe, expect, it } from 'vitest';
import { describeError, errorText, stderrReason } from '../lib/errors';

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
    expect(f.title).toBe('ffmpeg failed (exit code 1)');
    expect(f.message).toBe('boom');
    expect(f.details).toBe('boom');
  });
  it('maps the serialized job error code `external` like external_failure', () => {
    const stderr = [
      'Input #0, wav, from \'in.wav\':',
      '[libvorbis @ 0x55d0c1a2b3c0] encoder setup failed',
      'Error initializing output stream 0:0 -- Error while opening encoder for output stream #0:0 - maybe incorrect parameters such as bit_rate, rate, width or height',
      'Conversion failed!',
    ].join('\n');
    const f = describeError({ code: 'external', program: 'ffmpeg', exit_code: 1, stderr });
    expect(f.title).toBe('ffmpeg failed (exit code 1)');
    expect(f.message).toBe('libvorbis: encoder setup failed');
    expect(f.message).not.toMatch(/external/);
    expect(f.details).toBe(stderr);
    expect(errorText({ code: 'external', program: 'ffmpeg', exit_code: 1, stderr })).toBe(f.message);
  });
  it('falls back sensibly when an external failure has no stderr', () => {
    expect(describeError({ code: 'external', program: 'piper', exit_code: null, stderr: '' })).toMatchObject({
      title: 'piper failed', message: 'It stopped unexpectedly.', details: undefined,
    });
    expect(describeError({ code: 'external', program: 'qpdf', exit_code: 2 }).message).toBe('It exited without printing a reason.');
    expect(describeError({ code: 'external', message: 'Error: external' }).title).toBe('External program failed');
    expect(describeError({ code: 'external', message: 'pandoc failed (exit Some(64)): could not find data file' }).message).toMatch(/data file/);
  });
  it('picks the most telling stderr line', () => {
    expect(stderrReason('[mp3 @ 0xabc] Invalid sample rate 96000\nConversion failed!')).toBe('mp3: Invalid sample rate 96000');
    expect(stderrReason('working\nall done here')).toBe('all done here');
    expect(stderrReason("Unrecognized option 'fps_mode'.\nError splitting the argument list: Option not found")).toBe("Unrecognized option 'fps_mode'.");
    expect(stderrReason("convert: Not a TIFF or MDI file, bad magic number 20011 (0x4e2b). `in.tif' @ error/tiff.c/TIFFErrors/606.\nconvert: no images defined `out.tiff' @ error/convert.c/ConvertImageCommand/3229."))
      .toBe("convert: Not a TIFF or MDI file, bad magic number 20011 (0x4e2b). `in.tif'");
    expect(stderrReason('')).toBe('');
    expect(stderrReason(`Error: ${'x'.repeat(400)}`)).toHaveLength(300);
  });
  it('handles timeouts, permissions and unknown codes', () => {
    expect(describeError({ code: 'timeout', seconds: 30 }).message).toMatch(/30 seconds/);
    expect(describeError({ code: 'permission_denied', message: 'nope' }).title).toBe('Not allowed');
    expect(describeError({ code: 'weird' }).message).toBe('Error: weird');
    expect(describeError(undefined).title).toBe('Something went wrong');
  });
});
