import {describe,expect,it} from 'vitest';
import {actKinds,actRequest} from './ActorEpistemicActPanel.jsx';
const draft={subject_actor_id:'a',target_object_id:'q',act_kind:'DELEGATING',characterization:'REPORTED',delegate_actor_id:'b',context:'Interview',temporal_scope:'Before hearing',rationale:' Requested analyst inspect ledger ',basis_evidence_id:null,revises_event_id:null};
const actors=[{actor_id:'a'},{actor_id:'b'}],nodes=[{object_id:'q'}],evidence=[{evidence_id:'e'}];
const request=(d,events=[])=>actRequest(d,actors,nodes,evidence,events);
describe('attributed epistemic acts',()=>{
  it('describes actor conduct without sending instructions or inferring a state',()=>{
    expect(actKinds).toHaveLength(9);
    expect(request(draft)).toEqual({...draft,rationale:'Requested analyst inspect ledger'});
    expect(request({...draft,delegate_actor_id:null}).delegate_actor_id).toBeNull();
    expect(()=>request({...draft,act_kind:'EXECUTE'})).toThrow();
    expect(()=>request({...draft,act_kind:'IGNORING'})).toThrow();
    expect(()=>request({...draft,delegate_actor_id:'a'})).toThrow();
    expect(()=>request({...draft,delegate_actor_id:'foreign'})).toThrow();
  });
  it('requires visible evidence for an observed act and preserves exact revision identity',()=>{
    expect(()=>request({...draft,characterization:'OBSERVED'})).toThrow();
    expect(()=>request({...draft,characterization:'OBSERVED',basis_evidence_id:'foreign'})).toThrow();
    const prior={event_id:'prior',input:request(draft)};
    const revised={...draft,characterization:'OBSERVED',basis_evidence_id:'e',revises_event_id:'prior'};
    expect(request(revised,[prior])).toMatchObject({characterization:'OBSERVED',basis_evidence_id:'e',revises_event_id:'prior'});
    expect(()=>request({...revised,delegate_actor_id:null},[prior])).toThrow();
    expect(()=>request({...revised,temporal_scope:'Today'},[prior])).toThrow();
  });
});
