import {describe,expect,it} from 'vitest';
import {stateDimensions,orientationDimensions,trustDimensions,protectiveMotiveDimensions,stateRequest} from './ActorEpistemicStatePanel.jsx';
const draft={subject_actor_id:'a',target_object_id:'p',target_state_event_id:null,target_actor_id:null,dimension:'EXPOSURE',judgment:'INDICATED',context:'Interview',temporal_scope:'Before hearing',orientation_scope:null,orientation_goal:null,rationale:' Reported exposure ',basis_evidence_id:'',revises_event_id:null};
const actors=[{actor_id:'a'},{actor_id:'b'}],nodes=[{object_id:'p'}],evidence=[{evidence_id:'e'}];
const request=(d,events=[])=>stateRequest(d,actors,nodes,evidence,events);
describe('independent actor epistemic judgments',()=>{
  it('distinguishes ego protection from identity protection without promoting either to fact',()=>{
    expect(protectiveMotiveDimensions).toEqual(['EGO_PROTECTION','IDENTITY_PROTECTION']);
    const ego={...draft,dimension:'EGO_PROTECTION',judgment:'CONTESTED'};
    expect(request(ego).dimension).toBe('EGO_PROTECTION');
    expect(request({...ego,dimension:'IDENTITY_PROTECTION',judgment:'UNKNOWN'}).judgment).toBe('UNKNOWN');
    const prior={event_id:'ego',input:request(ego)};
    expect(()=>request({...ego,dimension:'IDENTITY_PROTECTION',revises_event_id:'ego'},[prior])).toThrow();
    expect(()=>request({...ego,target_object_id:null,target_state_event_id:'ego'},[prior])).toThrow();
    expect(()=>request({...ego,target_object_id:null,target_actor_id:'b'})).toThrow();
  });
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
  it('targets exact prior state events, including historical states, without an order limit',()=>{
    const prior={event_id:'prior',input:request(draft)};
    const revised={event_id:'revised',input:{...prior.input,revises_event_id:'prior'}};
    const next={...draft,target_object_id:null,target_state_event_id:'prior',dimension:'BELIEF'};
    expect(request(next,[prior,revised])).toMatchObject({target_object_id:null,target_state_event_id:'prior'});
    expect(()=>request({...next,target_object_id:'p'},[prior])).toThrow();
    expect(()=>request({...next,target_state_event_id:null},[prior])).toThrow();
    expect(()=>request({...next,target_state_event_id:'unknown'},[prior])).toThrow();
    expect(()=>request({...next,revises_event_id:'revised'},[prior,revised])).toThrow();
  });
  it('distinguishes orientations from states and requires goal and scope',()=>{
    expect(orientationDimensions).toHaveLength(8);
    const curiosity={...draft,dimension:'CURIOSITY',orientation_scope:'IMMEDIATE_REACTION',orientation_goal:' Clarify the ledger '};
    expect(request(curiosity)).toMatchObject({orientation_scope:'IMMEDIATE_REACTION',orientation_goal:'Clarify the ledger'});
    expect(request({...curiosity,dimension:'AVERSION',orientation_scope:'PERSISTENT_DISPOSITION',judgment:'UNKNOWN'}).judgment).toBe('UNKNOWN');
    expect(()=>request({...curiosity,orientation_scope:null})).toThrow();
    expect(()=>request({...curiosity,orientation_goal:' '})).toThrow();
    expect(()=>request({...curiosity,dimension:'BELIEF'})).toThrow();
    const prior={event_id:'curiosity',input:request(curiosity)};
    expect(()=>request({...curiosity,revises_event_id:'curiosity',orientation_scope:'PERSISTENT_DISPOSITION'},[prior])).toThrow();
    expect(()=>request({...curiosity,revises_event_id:'curiosity',orientation_goal:'Other goal'},[prior])).toThrow();
  });
  it('preserves directed independent trust and explicitly distinct distrust, with qualified loyalty targets',()=>{
    expect(trustDimensions).toHaveLength(3);
    const competence={...draft,target_object_id:null,target_actor_id:'b',dimension:'TRUST_COMPETENCE'};
    expect(request(competence)).toMatchObject({subject_actor_id:'a',target_actor_id:'b',dimension:'TRUST_COMPETENCE',judgment:'INDICATED'});
    expect(request({...competence,dimension:'TRUST_CONFIDENTIALITY',judgment:'DISTRUST_INDICATED'}).judgment).toBe('DISTRUST_INDICATED');
    expect(()=>request({...draft,dimension:'TRUST_COMPETENCE'})).toThrow();
    expect(()=>request({...competence,dimension:'BELIEF'})).toThrow();
    expect(()=>request({...competence,dimension:'LOYALTY',judgment:'DISTRUST_INDICATED'})).toThrow();
    expect(request({...draft,dimension:'LOYALTY'}).target_object_id).toBe('p');
    expect(request({...competence,dimension:'LOYALTY'}).target_actor_id).toBe('b');
    const prior={event_id:'trust',input:request(competence)};
    expect(()=>request({...competence,revises_event_id:'trust',target_actor_id:'a'},[prior])).toThrow();
  });
});
