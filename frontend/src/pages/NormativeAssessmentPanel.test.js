import {describe,expect,it} from 'vitest';
import {normativeDimensions,normativeRequest} from './NormativeAssessmentPanel.jsx';
const actors=[{actor_id:'subject'},{actor_id:'supervisor'},{actor_id:'institution'}];
const nodes=[{object_id:'action',kind:'PROPOSITION'},{object_id:'order',kind:'PROPOSITION'},{object_id:'policy',kind:'HYPOTHESIS'},{object_id:'question',kind:'QUESTION'}];
const evidence=[{evidence_id:'e'}];
const base={subject_actor_id:'subject',action_object_id:'action',norm_object_id:'order',issuer_actor_id:'supervisor',competing_norm_object_id:null,competing_issuer_actor_id:null,dimension:'PROHIBITION',judgment:'INDICATED',context:' Audit disclosure ',temporal_scope:'At disclosure',rationale:' Reported order ',basis_evidence_id:'e',revises_event_id:null};
const request=(draft,events=[])=>normativeRequest(draft,actors,nodes,evidence,events);

describe('independent normative assessment',()=>{
  it('preserves every independent dimension and distinguishes one norm from another',()=>{
    expect(normativeDimensions).toEqual(['APPLICABILITY','AUTHORIZATION','OBLIGATION','PROHIBITION','COMPLIANCE','VIOLATION','PRECEDENCE','EXCEPTION']);
    for(const dimension of normativeDimensions.slice(0,6))expect(request({...base,dimension,judgment:'CONTESTED'})).toMatchObject({dimension,judgment:'CONTESTED',competing_norm_object_id:null});
    expect(request(base)).toEqual({...base,context:'Audit disclosure',rationale:'Reported order'});
    expect(()=>request({...base,norm_object_id:'action'})).toThrow();
    expect(()=>request({...base,norm_object_id:'question'})).toThrow();
    expect(()=>request({...base,issuer_actor_id:'foreign'})).toThrow();
    expect(()=>request({...base,basis_evidence_id:'foreign'})).toThrow();
  });
  it('requires distinct norm and issuer for precedence and exception, never for independent judgments',()=>{
    for(const dimension of ['PRECEDENCE','EXCEPTION']){
      expect(()=>request({...base,dimension})).toThrow();
      const comparison={...base,dimension,competing_norm_object_id:'policy',competing_issuer_actor_id:'institution'};
      expect(request(comparison)).toMatchObject({dimension,competing_norm_object_id:'policy',competing_issuer_actor_id:'institution'});
      expect(()=>request({...comparison,competing_norm_object_id:'order'})).toThrow();
      expect(()=>request({...comparison,competing_issuer_actor_id:null})).toThrow();
      expect(()=>request({...comparison,dimension:'AUTHORIZATION'})).toThrow();
    }
  });
  it('locks every identity pin for revision and does not silently supersede competing records',()=>{
    const prior={event_id:'prior',input:request(base)};
    const revised={...base,revises_event_id:'prior',judgment:'UNKNOWN',rationale:'Further investigation needed'};
    expect(request(revised,[prior])).toMatchObject({judgment:'UNKNOWN',revises_event_id:'prior'});
    for(const changes of [{norm_object_id:'policy'},{issuer_actor_id:'institution'},{dimension:'VIOLATION'},{temporal_scope:'Later'}])expect(()=>request({...revised,...changes},[prior])).toThrow();
    expect(()=>request(revised,[prior,{event_id:'revision',input:request(revised,[prior])}])).toThrow();
    expect(request({...base,judgment:'CONTESTED'},[prior]).revises_event_id).toBeNull();
    expect(()=>request({...base,context:'é'.repeat(1025)})).toThrow();
  });
});
