import {describe,expect,it} from 'vitest';
import {comparisonQuestions,inquiryCriteria,inquiryDecisionRequest} from './inquiryComparison.js';

const event=(id,question,dimension,result,revises=null)=>({event_id:id,recorded_by:'actor',input:{target:{kind:'OBJECT',object_id:question},dimension,result,confidence:'LOW',criterion:'criterion',context:'context',rationale:'rationale',revises_event_id:revises}});
const question=(id,criteria=[])=>({question_object_id:id,current_relevance_events:[],current_discriminatory_power_events:[],selection_criteria:criteria,addressed_gaps:[],tested_hypotheses:[],presuppositions:[]});
const base={nodes:['a','b','c','d'].map(id=>({kind:'QUESTION',object_id:id,text:id})),question_inquiries:['a','b','c','d'].map(id=>question(id)),assessments:[]};

describe('investigator-selected question comparison',()=>{
  it('keeps selection order and discards duplicates, unavailable IDs and excess choices without ranking',()=>{
    expect(comparisonQuestions(base,['c','missing','a','c','b','d']).map(c=>c.question.object_id)).toEqual(['c','a','b']);
    expect(comparisonQuestions(base,[])).toEqual([]);
  });
  it('preserves conflicting current judgments and leaves absence separate from unknown',()=>{
    const q=question('a',[{dimension:'URGENCY',current_events:['new','competing']},{dimension:'ACCESSIBILITY',current_events:['unknown']},{dimension:'ACQUISITION_COST',current_events:[]}]);
    const snapshot={...base,question_inquiries:[q,question('b')],assessments:[event('old','a','URGENCY','URGENT'),event('new','a','URGENCY','DEFERRABLE','old'),event('competing','a','URGENCY','URGENT'),event('unknown','a','ACCESSIBILITY','UNKNOWN')]};
    const a=comparisonQuestions(snapshot,['a'])[0];
    expect(a.criteria).toHaveLength(inquiryCriteria.length);
    const criterion=d=>a.criteria.find(c=>c.dimension===d);
    expect(criterion('URGENCY').judgments.map(e=>e.event_id)).toEqual(['new','competing']);
    expect(criterion('ACCESSIBILITY').judgments[0].input.result).toBe('UNKNOWN');
    expect(criterion('ACQUISITION_COST')).toMatchObject({judgments:[],projectionIncomplete:false});
    expect(criterion('CONTAMINATION_RISK')).toMatchObject({judgments:[],projectionIncomplete:true});
  });
  it('does not render stale, foreign, wrong-dimension or unknown event references',()=>{
    const q=question('a',[{dimension:'URGENCY',current_events:['old','foreign','wrong','missing','valid','valid']}]);
    const snapshot={...base,question_inquiries:[q],assessments:[event('old','a','URGENCY','URGENT'),event('valid','a','URGENCY','DEFERRABLE','old'),event('foreign','b','URGENCY','URGENT'),event('wrong','a','ACCESSIBILITY','ACCESSIBLE')]};
    const criterion=comparisonQuestions(snapshot,['a'])[0].criteria.find(c=>c.dimension==='URGENCY');
    expect(criterion.projectionIncomplete).toBe(true);
    expect(criterion.judgments.map(e=>e.event_id)).toEqual(['valid']);
  });
  it('records an explicit decision with only chosen historical event identities',()=>{
    const q=question('a',[{dimension:'URGENCY',current_events:['valid']}]);
    const columns=comparisonQuestions({...base,question_inquiries:[q,question('b')],assessments:[event('valid','a','URGENCY','URGENT')]},['b','a']);
    const draft={question:'b',disposition:'PURSUE_NEXT',rationale:' Inspect the log first. '};
    expect(inquiryDecisionRequest(columns,draft,['valid'])).toEqual({question_id:'b',considered_question_ids:['b','a'],assessment_event_ids:['valid'],disposition:'PURSUE_NEXT',rationale:'Inspect the log first.'});
    expect(inquiryDecisionRequest(columns,draft,[]).assessment_event_ids).toEqual([]);
    expect(()=>inquiryDecisionRequest(columns,{...draft,question:'foreign'},[])).toThrow();
    expect(()=>inquiryDecisionRequest(columns,{...draft,disposition:'EXECUTE'},[])).toThrow();
    expect(()=>inquiryDecisionRequest(columns,draft,['stale'])).toThrow();
    expect(()=>inquiryDecisionRequest(columns,draft,['valid','valid'])).toThrow();
  });
});
