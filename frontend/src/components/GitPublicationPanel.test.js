import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';

const source = readFileSync(new URL('./GitPublicationPanel.jsx', import.meta.url), 'utf8');

describe('GitPublicationPanel safety boundary', () => {
  it('never presents a local target or intent as a remote publication', () => {
    expect(source).toContain('no upload yet');
    expect(source).toContain('does not');
    expect(source).toContain('connect to GitHub');
    expect(source).toContain('Remote publish performed: no');
    expect(source).toContain('GitHub has not been contacted.');
  });

  it('records exact scope before future provider authorization', () => {
    expect(source).toContain('package_sha256');
    expect(source).toContain('requested_ref');
    expect(source).toContain('Record publication intent — does not upload');
    expect(source).toContain('Provider connection verified: no');
  });

  it('reuses IDs for interrupted target and intent responses', () => {
    expect(source).toContain('pendingTarget.current?.signature !== signature');
    expect(source).toContain('pendingIntent.current?.signature !== signature');
    expect(source).toContain('pendingTarget.current.targetId');
    expect(source).toContain('pendingIntent.current.publicationId');
  });
});
