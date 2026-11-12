import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';

const source = readFileSync(new URL('./GroundedScorePanel.jsx', import.meta.url), 'utf8');

describe('GroundedScorePanel product boundary', () => {
  it('uses stable retry IDs and the explicit immutable-record endpoint', () => {
    expect(source).toContain('pending.current?.signature !== signature');
    expect(source).toContain('pending.current.scoreId');
    expect(source).toContain('client.recordGroundedScore(');
    expect(source).toContain('Record immutable heuristic score');
  });

  it('sends human strengths and rationale but leaves stance and provenance derivation to the backend', () => {
    expect(source).toContain('source_provenance');
    expect(source).toContain('declared_strength: strength');
    expect(source).toContain('rationale');
    expect(source).not.toContain('documented_origin_id:');
    expect(source).not.toContain('stance:');
  });

  it('makes non-truth semantics and bounded input visible', () => {
    expect(source).toContain('not a probability of truth');
    expect(source).toContain('published analysis is unchanged');
    expect(source).toContain('more than 64 linked source fragments');
    expect(source).toContain('probability of truth: no');
  });
});
