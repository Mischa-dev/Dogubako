import { describe, expect, it, vi, beforeEach } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import type { Artifact } from '../lib/types';

const backend: Record<string, any> = {};
vi.mock('../lib/app', () => ({ useApp: () => ({ backend }) }));
vi.mock('../lib/backend', () => ({ asServer: (b: any) => (b.mode === 'server' ? b : null) }));

import { ArtifactList } from '../results/Artifacts';

const arts: Artifact[] = [
  { name: 'tile-1.png', mime: 'image/png', size: 10, path: '/out/run/tile-1.png' },
  { name: 'tile-2.png', mime: 'image/png', size: 10, path: '/out/run/tile-2.png' },
  { name: 'readme.txt', mime: 'application/octet-stream', size: 3, path: '/out/run/readme.txt', rel_path: 'docs/readme.txt', label: 'docs/readme.txt' },
];

beforeEach(() => {
  for (const k of Object.keys(backend)) delete backend[k];
  Object.assign(backend, { artifactUrl: () => 'about:blank', downloadArtifact: vi.fn() });
});

describe('ArtifactList save all', () => {
  it('desktop: one click saves every file with its folder', async () => {
    backend.mode = 'desktop';
    backend.saveArtifacts = vi.fn().mockResolvedValue({ saved: 3, dir: '/home/me/out' });
    render(<ArtifactList artifacts={arts} collapsed />);
    expect(screen.getByText('Save all keeps these folders.')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: /Save all/ }));
    await waitFor(() => expect(backend.saveArtifacts).toHaveBeenCalledTimes(1));
    const passed: Artifact[] = backend.saveArtifacts.mock.calls[0][0];
    expect(passed.map((a) => a.name)).toEqual(['tile-1.png', 'tile-2.png', 'readme.txt']);
    expect(passed[2].rel_path).toBe('docs/readme.txt');
  });

  it('desktop: a single output has no Save all', () => {
    backend.mode = 'desktop';
    backend.saveArtifacts = vi.fn();
    render(<ArtifactList artifacts={arts.slice(0, 1)} collapsed />);
    expect(screen.queryByRole('button', { name: /Save all/ })).toBeNull();
  });

  it('server: offers the job zip, which keeps folders', () => {
    backend.mode = 'server';
    backend.zipUrl = (id: string) => `/api/v1/jobs/${id}/artifacts.zip`;
    backend.downloadUrl = () => '/x';
    render(<ArtifactList artifacts={arts} jobId="job1" collapsed />);
    const link = screen.getByRole('link', { name: /Download all/ });
    expect(link.getAttribute('href')).toBe('/api/v1/jobs/job1/artifacts.zip');
    expect(screen.getByText('Download all keeps these folders.')).toBeTruthy();
    expect(screen.queryByRole('button', { name: /Save all/ })).toBeNull();
  });
});
