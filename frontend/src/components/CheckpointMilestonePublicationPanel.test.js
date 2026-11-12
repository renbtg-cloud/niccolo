import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';

const source = readFileSync(new URL('./CheckpointMilestonePublicationPanel.jsx', import.meta.url), 'utf8');

describe('checkpoint milestone Git publication UI', () => {
  it('binds preview and immutable intent to the exact milestone package', () => {
    expect(source).toContain('operationCheckpointGitPublicationPreview');
    expect(source).toContain('milestone.milestone_id');
    expect(source).toContain('recordOperationCheckpointGitPublicationIntent');
    expect(source).toContain('Record milestone publication intent — no upload');
  });

  it('only offers targets declared private', () => {
    expect(source).toContain("target.repository_visibility === 'PRIVATE'");
    expect(source).toContain('public/unknown targets are intentionally excluded here');
  });

  it('keeps publication explicit, transiently authorized, and remotely verified', () => {
    expect(source).toContain('Transient GitHub fine-grained token');
    expect(source).toContain('I authorize publishing this exact milestone package');
    expect(source).toContain('publishOperationCheckpointGitPublicationIntent');
    expect(source).toContain('remote_publish_verified');
    expect(source).toContain('provider credentials and response bodies are not persisted');
  });

  it('never upgrades a provisional milestone by publishing it', () => {
    expect(source).toContain('This checkpoint is provisional.');
    expect(source).toContain('It remains provisional after verified publication.');
    expect(source).toContain('does not make the checkpoint true, complete, or final');
  });
});
