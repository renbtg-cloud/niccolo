import { describe, expect, it } from 'vitest';
import { boundedInteger, comparisonRequest, initialPlaygroundSettings } from './epistemicPlayground.js';

const situation = 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa';

describe('epistemic what-if playground boundaries', () => {
  it('uses three fictional evidence references and never supplies an instruction to persist', () => {
    const x = comparisonRequest(initialPlaygroundSettings, situation);
    expect(x.situation_id).toBe(situation);
    expect(x.trial_policy).toEqual({
      support_multiplier_pct: 100,
      counter_multiplier_pct: 100,
      per_origin_cap: 100,
    });
    expect(x.human_declared_confidence_percent).toBe(90);
    expect(x.observations).toHaveLength(3);
    expect(x.observations[0].documented_origin_id).toBe(x.observations[1].documented_origin_id);
    expect(x.observations[2].stance).toBe('CONTRADICTS');
    expect(x).not.toHaveProperty('persist');
    expect(x).not.toHaveProperty('verified_evidence');
    expect(x).not.toHaveProperty('baseline_policy');
  });
  it('exposes source duplication as user-controlled rather than pretending independence', () => {
    const x = comparisonRequest({ ...initialPlaygroundSettings, sharedOrigin: false }, situation);
    expect(x.observations[0].documented_origin_id).not.toBe(x.observations[1].documented_origin_id);
    expect(x.observations[0].stance).toBe('SUPPORTS');
    expect(x.observations[1].stance).toBe('SUPPORTS');
  });
  it('keeps the human belief independent from the evidence weights and accepts unknown', () => {
    const unknown = comparisonRequest({ ...initialPlaygroundSettings, humanConfidence: '' }, situation);
    expect(unknown.human_declared_confidence_percent).toBeNull();
    const certain = comparisonRequest({ ...initialPlaygroundSettings, humanConfidence: '100' }, situation);
    expect(certain.human_declared_confidence_percent).toBe(100);
    expect(certain.observations).toEqual(unknown.observations);
  });
  it('rejects fractional, negative, over-bound and empty numbers', () => {
    expect(boundedInteger('0', 0, 100, 'sample')).toBe(0);
    expect(() => boundedInteger('101', 0, 100, 'sample')).toThrow();
    expect(() => boundedInteger('-1', 0, 100, 'sample')).toThrow();
    expect(() => boundedInteger('9.5', 0, 100, 'sample')).toThrow();
    expect(() => boundedInteger('', 0, 100, 'sample')).toThrow();
    expect(() => comparisonRequest({ ...initialPlaygroundSettings, counterMultiplier: '201' }, situation)).toThrow();
    expect(() => comparisonRequest({ ...initialPlaygroundSettings, perOriginCap: '101' }, situation)).toThrow();
    expect(() => comparisonRequest({ ...initialPlaygroundSettings, firstStrength: '101' }, situation)).toThrow();
  });
});
