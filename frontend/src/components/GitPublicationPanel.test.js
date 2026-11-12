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
    expect(source).toContain('report success only after the remote commit SHA matches');
  });

  it('records exact scope before future provider authorization', () => {
    expect(source).toContain('package_sha256');
    expect(source).toContain('requested_ref');
    expect(source).toContain('Record publication intent — does not upload');
    expect(source).toContain('Provider connection verified: no');
  });

  it('keeps provider authorization transient and explicit', () => {
    expect(source).toContain('type="password"');
    expect(source).toContain('autoComplete="off"');
    expect(source).toContain('Transient token cleared from this UI.');
    expect(source).toContain('setGithubToken(\'\')');
    expect(source).toContain('I authorize publishing this exact standard package');
    expect(source).toContain('pendingPublish.current?.signature !== signature');
    expect(source).not.toContain('localStorage');
  });

  it('shows a secret-free durable remote event trail', () => {
    expect(source).toContain('Remote publication event log');
    expect(source).toContain('gitPublicationRemoteEvents');
    expect(source).toContain('never GitHub tokens or provider response bodies');
    expect(source).toContain('last_failure_class');
  });

  it('reuses IDs for interrupted target and intent responses', () => {
    expect(source).toContain('pendingTarget.current?.signature !== signature');
    expect(source).toContain('pendingIntent.current?.signature !== signature');
    expect(source).toContain('pendingTarget.current.targetId');
    expect(source).toContain('pendingIntent.current.publicationId');
  });
});
