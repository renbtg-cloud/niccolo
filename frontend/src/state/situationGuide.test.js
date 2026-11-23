import { describe, expect, it } from 'vitest';
import { situationGuide } from './situationGuide.js';

describe('Situation guide', () => {
  it('reports transparent counts without creating a readiness gate', () => {
    expect(situationGuide({
      focus: 'Understand what happened',
      contributions: [{ contribution_id: 'c' }],
    }, {artifactCount:2,representationCount:1,relationshipCount:1})).toEqual({
      focusSet: true,
      artifactCount: 2,
      representationCount: 1,
      relationshipCount: 1,
      contextCount: 1,
      hasGroundingMaterial: true,
      analysisBlocked: false,
    });
  });
  it('still permits a first pass with focus alone', () => {
    const view=situationGuide({ focus:'Question', contributions:[] });
    expect(view.hasGroundingMaterial).toBe(false);
    expect(view.analysisBlocked).toBe(false);
  });
});
