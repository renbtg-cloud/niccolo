import { describe, expect, it } from 'vitest';
import { situationGuide } from './situationGuide.js';

describe('Situation guide', () => {
  it('reports transparent counts without creating a readiness gate', () => {
    expect(situationGuide({
      focus: 'Understand what happened',
      source_document_ids: ['a','b'],
      contributions: [{ contribution_id: 'c' }],
    })).toEqual({
      focusSet: true,
      sourceCount: 2,
      contextCount: 1,
      hasGroundingMaterial: true,
      analysisBlocked: false,
    });
  });
  it('still permits a first pass with focus alone', () => {
    const view=situationGuide({ focus:'Question', source_document_ids:[], contributions:[] });
    expect(view.hasGroundingMaterial).toBe(false);
    expect(view.analysisBlocked).toBe(false);
  });
});
