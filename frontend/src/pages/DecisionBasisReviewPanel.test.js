import {describe,expect,it} from 'vitest';
import {decisionBasisReviewRequest} from './DecisionBasisReviewPanel.jsx';

describe('decision basis review',()=>{
  const change={original_assessment_event_id:'old',current_assessment_event_id:'new',decision_id:'d'};
  it('pins both exact assessment versions without rewriting the decision',()=>{
    expect(decisionBasisReviewRequest(change,'RETAINED',' reason ')).toEqual({original_assessment_event_id:'old',reviewed_assessment_event_id:'new',disposition:'RETAINED',note:'reason'});
    expect(decisionBasisReviewRequest(change,'DEFERRED','reason').disposition).toBe('DEFERRED');
  });
  it('rejects missing or identical versions, execution dispositions and empty notes',()=>{
    expect(()=>decisionBasisReviewRequest({...change,current_assessment_event_id:'old'},'RETAINED','reason')).toThrow();
    expect(()=>decisionBasisReviewRequest({...change,current_assessment_event_id:null},'RETAINED','reason')).toThrow();
    expect(()=>decisionBasisReviewRequest(change,'EXECUTE','reason')).toThrow();
    expect(()=>decisionBasisReviewRequest(change,'DEFERRED',' ')).toThrow();
  });
});
