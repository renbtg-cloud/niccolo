import { describe, expect, it } from 'vitest';
import { analysisHistoryLabel, historyGroups } from './analysisHistoryPresentation.js';

describe('analysis history presentation', () => {
  it('shows direct reanalysis lineage without inferring missing links', () => {
    const rows = historyGroups([
      { analysis_id:'a2', reanalysis_of:'a1', answer_text:'new', input_is_current:true },
      { analysis_id:'orphan', reanalysis_of:'missing', answer_text:null, input_is_current:true },
      { analysis_id:'a1', reanalysis_of:null, answer_text:'old', input_is_current:false },
    ]);
    expect(rows.map(x => [x.analysis_id,x.depth])).toEqual([
      ['orphan',0], ['a1',0], ['a2',1],
    ]);
    expect(analysisHistoryLabel(rows[2])).toBe('Current input');
    expect(analysisHistoryLabel(rows[1])).toBe('Historical input preserved');
    expect(analysisHistoryLabel(rows[0])).toBe('No published answer');
  });
});
