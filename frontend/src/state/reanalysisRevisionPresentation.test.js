import { describe, expect, it } from 'vitest';
import { reanalysisChanges } from './reanalysisRevisionPresentation.js';

describe('reanalysis revision presentation', () => {
  it('shows only recorded changes and does not manufacture causality', () => {
    const rows = reanalysisChanges({
      focus_changed: true,
      added_source_count: 2,
      removed_source_count: 0,
      added_context_count: 1,
      removed_context_count: 0,
      retained: 3,
      refined: 1,
      reversed: 0,
      dropped: 0,
      split: 0,
      merged: 0,
      input_change_proves_causal_reason_for_output_change: false,
    });
    expect(rows).toEqual([
      {label:'Focus changed',count:1},
      {label:'Sources added',count:2},
      {label:'Human context added',count:1},
      {label:'Claims retained',count:3},
      {label:'Claims refined',count:1},
    ]);
  });
});
