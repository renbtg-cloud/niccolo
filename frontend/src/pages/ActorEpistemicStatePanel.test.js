import {describe,expect,it} from 'vitest';
import {stateDimensions,stateRequest} from './ActorEpistemicStatePanel.jsx';
const draft={subject_actor_id:'a',target_object_id:'p',dimension:'EXPOSURE',judgment:'INDICATED',context:'Interview',temporal_scope:'Before hearing',rationale:' Reported exposure ',basis_evidence_id:'',revises_event_id:null};
const actors=[{actor_id:'a'}],nodes=[{object_id:'p'}],evidence=[{evidence_id:'e'}];
const request=(d,events=[])=>stateRequest(d,actors,nodes,evidence,events);
describe('independent actor epistemic judgments',()=>{
  it('keeps dimensions distinct without deriving belief or knowledge from exposure',()=>{
    expect(stateDimensions).toHaveLength(15);
    expect(request(draft)).toEqual({...draft,rationale:'Reported exposure',basis_evidence_id:null});
    expect(request({...draft,dimension:'BELIEF',judgment:'UNKNOWN'}).judgment).toBe('UNKNOWN');
    expect(()=>request({...draft,dimension:'KNOWS'})).toThrow();
  });
  it('requires visible identities and evidence for explicitly supported knowledge',()=>{
    expect(()=>request({...draft,subject_actor_id:'foreign'})).toThrow();
    expect(()=>request({...draft,target_object_id:'foreign'})).toThrow();
    expect(()=>request({...draft,dimension:'EVIDENCE_SUPPORTED_KNOWLEDGE'})).toThrow();
    expect(request({...draft,dimension:'EVIDENCE_SUPPORTED_KNOWLEDGE',basis_evidence_id:'e'}).basis_evidence_id).toBe('e');
  });
  it('requires a current exact revision preserving context, temporal scope and dimension',()=>{
    const prior={event_id:'prior',input:request(draft)};
    const next={...draft,revises_event_id:'prior',judgment:'CONTESTED'};
    expect(request(next,[prior]).revises_event_id).toBe('prior');
    expect(()=>request({...next,dimension:'BELIEF'},[prior])).toThrow();
    expect(()=>request({...next,temporal_scope:'Today'},[prior])).toThrow();
    expect(()=>request(next,[prior,{event_id:'next',input:next}])).toThrow();
  });
});
