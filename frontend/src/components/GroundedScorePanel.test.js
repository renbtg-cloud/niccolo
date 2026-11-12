import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';

const source = readFileSync(new URL('./GroundedScorePanel.jsx', import.meta.url), 'utf8');

describe('GroundedScorePanel product boundary', () => {
  it('makes live evaluation unmistakably different from immutable history', () => {
    expect(source).toContain('Preview live heuristic score');
    expect(source).toContain('Live preview — not recorded');
    expect(source).toContain('Record exactly this preview');
    expect(source).toContain('Recorded score artifacts — immutable history');
    expect(source).toContain('not an immutable audit artifact');
  });

  it('cannot record stale inputs after the previewed draft changes', () => {
    expect(source).toContain('previewSignature !== currentSignature');
    expect(source).toContain('Preview the current inputs before recording them.');
    expect(source).toContain('setPreviewSignature(null)');
  });
});
