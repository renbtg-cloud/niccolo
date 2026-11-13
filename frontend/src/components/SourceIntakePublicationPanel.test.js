import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';

const source = readFileSync(new URL('./SourceIntakePublicationPanel.jsx', import.meta.url), 'utf8');

describe('received-source Git publication panel', () => {
  it('stays dormant until the user opens the receipt publication details', () => {
    expect(source).toContain('enabled: !!client && open');
    expect(source).toContain('onToggle={(event) => setOpen(event.currentTarget.open)}');
  });

  it('lists only recorded private GitHub targets', () => {
    expect(source).toContain("filter((target) => target.repository_visibility === 'PRIVATE')");
    expect(source).toContain('Public or unknown targets are intentionally excluded.');
  });

  it('keeps RECEIVED distinct from authenticity, truth, completeness and analysis', () => {
    expect(source).toContain('The source still remains RECEIVED.');
    expect(source).toContain('Authenticity established: no · truth established: no · completeness proved: no.');
    expect(source).toContain('does not establish authenticity, truth, completeness, independence, or analysis');
  });

  it('records intent before any remote publication and requires explicit transient authorization', () => {
    expect(source).toContain('Record receipt publication intent — no upload');
    expect(source).toContain('Publication intent recorded.');
    expect(source).toContain('Transient GitHub fine-grained token');
    expect(source).toContain('I authorize publishing this exact RECEIVED-source receipt');
    expect(source).toContain('publishIntakeGitPublicationIntent(');
  });

  it('clears transient authorization after success or failure and preserves retry identity', () => {
    expect(source).toContain("setGithubToken('')");
    expect(source).toContain('pendingPublish.current?.signature !== signature');
    expect(source).toContain('retry keeps the recorded intent');
  });

  it('reports verified remote publication without epistemic upgrade', () => {
    expect(source).toContain('Verified published.');
    expect(source).toContain('remote ref points at the verified commit for this exact receipt package');
    expect(source).toContain('Only bounded status/commit/ref metadata is stored');
  });
});
