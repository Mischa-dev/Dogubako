import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import type { JobView } from '../lib/types';
import { tool } from './fixtures';

const job: JobView = {
  id: 'j_1',
  tool_id: 'image.qr',
  status: 'succeeded',
  created_at: 0,
  params: {},
  result: {
    text: null,
    data: null,
    artifacts: [{ ref: 'f_1', name: 'qr.png', size: 10, mime: 'image/png' }],
    warnings: ['The built-in reader could not decode this code; test it before printing'],
    duration_ms: 5,
  },
} as unknown as JobView;

vi.mock('../lib/app', () => ({ useApp: () => ({ backend: { artifactUrl: () => 'about:blank', fileUrl: () => 'about:blank' } }) }));
vi.mock('../tools/useToolForm', () => ({
  useToolForm: () => ({ values: {}, shownErrors: {}, setValue: () => undefined, job: { job, running: false } }),
}));
vi.mock('../tools/GenericTool', () => ({ useLiveRun: () => undefined, useToolShortcuts: () => undefined }));
vi.mock('../results/Artifacts', () => ({ ArtifactList: () => null }));

const { default: Qr } = await import('../tools/special/Qr');

describe('Qr', () => {
  it('shows backend warnings next to the preview', () => {
    render(<Qr tool={tool({ id: 'image.qr', name: 'QR Code' })} />);
    expect(screen.getByText(/could not decode this code/)).toBeTruthy();
  });
});
