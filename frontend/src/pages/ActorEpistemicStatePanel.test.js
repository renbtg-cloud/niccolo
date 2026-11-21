import {describe,expect,it} from 'vitest';
import {stateDimensions,orientationDimensions,trustDimensions,protectiveMotiveDimensions,autonomyDimensions,contextualPressureDimensions,threatFacetDimensions,contextualVulnerabilityDimensions,influenceDimensions,influenceTargets,powerDimensions,asymmetryDimensions,stateRequest} from './ActorEpistemicStatePanel.jsx';
const draft={subject_actor_id:'a',target_object_id:'p',target_state_event_id:null,target_actor_id:null,counterparty_actor_id:null,influence_target:null,dimension:'EXPOSURE',judgment:'INDICATED',context:'Interview',temporal_scope:'Before hearing',orientation_scope:null,orientation_goal:null,rationale:' Reported exposure ',basis_evidence_id:'',revises_event_id:null};
const actors=[{actor_id:'a'},{actor_id:'b'}],nodes=[{object_id:'p'}],evidence=[{evidence_id:'e'}];
const request=(d,events=[])=>stateRequest(d,actors,nodes,evidence,events);
describe('independent actor epistemic judgments',()=>{
  it('distinguishes power, authority, and five exact directed comparative advantages',()=>{
    expect(powerDimensions).toHaveLength(7);
    expect(asymmetryDimensions).toHaveLength(5);
    expect(request({...draft,dimension:'EPISTEMIC_POWER'}).counterparty_actor_id).toBeNull();
    expect(request({...draft,dimension:'FORMAL_AUTHORITY',judgment:'NOT_INDICATED'}).judgment).toBe('NOT_INDICATED');
    for(const dimension of asymmetryDimensions){
      const item={...draft,dimension,counterparty_actor_id:'b'};
      expect(request(item).counterparty_actor_id).toBe('b');
      expect(()=>request({...item,counterparty_actor_id:null})).toThrow();
      expect(()=>request({...item,counterparty_actor_id:'a'})).toThrow();
      expect(()=>request({...item,subject_actor_id:'b',counterparty_actor_id:'a',revises_event_id:'r'},[{event_id:'r',input:request(item)}])).toThrow();
    }
  });
  it('qualifies influence on nine independent targets without inferring the actual effect',()=>{
    expect(influenceDimensions).toHaveLength(5);
    expect(influenceTargets).toHaveLength(9);
    const act={...draft,dimension:'INFLUENCE_ACT',counterparty_actor_id:'b',influence_target:'ATTENTION'};
    expect(request(act)).toMatchObject({counterparty_actor_id:'b',influence_target:'ATTENTION'});
    expect(request({...act,dimension:'INFLUENCE_ACTUAL_EFFECT',counterparty_actor_id:null,judgment:'UNKNOWN'}).judgment).toBe('UNKNOWN');
    expect(()=>request({...act,influence_target:null})).toThrow();
    expect(()=>request({...act,counterparty_actor_id:null})).toThrow();
    expect(()=>request({...draft,influence_target:'ATTENTION'})).toThrow();
    const prior={event_id:'act',input:request(act)};
    expect(()=>request({...act,revises_event_id:'act',influence_target:'BELIEF'},[prior])).toThrow();
  });
  it('distinguishes contextual vulnerability, susceptibility, compliance, consent and submission',()=>{
    expect(contextualVulnerabilityDimensions).toHaveLength(5);
    const compliance={...draft,dimension:'COMPLIANCE',counterparty_actor_id:'b'};
    expect(request(compliance).judgment).toBe('INDICATED');
    expect(request({...compliance,dimension:'CONSENT',judgment:'NOT_INDICATED'}).judgment).toBe('NOT_INDICATED');
    expect(request({...compliance,dimension:'VULNERABILITY',counterparty_actor_id:null}).target_object_id).toBe('p');
    const prior={event_id:'compliance',input:request(compliance)};
    expect(()=>request({...compliance,dimension:'CONSENT',revises_event_id:'compliance'},[prior])).toThrow();
  });
  it('requires an alleged issuer for threat capability or intent, without inferring credibility or outcome',()=>{
    expect(threatFacetDimensions).toHaveLength(4);
    const capability={...draft,dimension:'THREAT_CAPABILITY',counterparty_actor_id:'b'};
    expect(request(capability).dimension).toBe('THREAT_CAPABILITY');
    expect(()=>request({...capability,counterparty_actor_id:null})).toThrow();
    expect(()=>request({...capability,dimension:'THREAT_INTENT',counterparty_actor_id:null})).toThrow();
    expect(request({...capability,dimension:'THREAT_PERCEIVED_CREDIBILITY',counterparty_actor_id:null,judgment:'UNKNOWN'}).judgment).toBe('UNKNOWN');
    expect(request({...capability,dimension:'THREAT_OUTCOME',counterparty_actor_id:null,judgment:'NOT_INDICATED'}).judgment).toBe('NOT_INDICATED');
  });
  it('pins an optional registered counterparty without changing the pressure target',()=>{
    const pressure={...draft,dimension:'PRESSURE',counterparty_actor_id:'b'};
    expect(request(pressure)).toMatchObject({target_object_id:'p',counterparty_actor_id:'b'});
    expect(()=>request({...pressure,counterparty_actor_id:'a'})).toThrow();
    expect(()=>request({...pressure,counterparty_actor_id:'foreign'})).toThrow();
    expect(()=>request({...pressure,dimension:'BELIEF'})).toThrow();
    const prior={event_id:'pressure',input:request(pressure)};
    expect(()=>request({...pressure,revises_event_id:'pressure',counterparty_actor_id:null},[prior])).toThrow();
  });
  it('does not infer coercion from a threat, incentive, constraint or dependency',()=>{
    expect(contextualPressureDimensions).toHaveLength(8);
    const threat={...draft,dimension:'COMMUNICATED_THREAT',judgment:'INDICATED'};
    expect(request(threat).dimension).toBe('COMMUNICATED_THREAT');
    expect(request({...threat,dimension:'COERCION',judgment:'UNKNOWN'}).judgment).toBe('UNKNOWN');
    const prior={event_id:'threat',input:request(threat)};
    expect(()=>request({...threat,dimension:'COERCION',revises_event_id:'threat'},[prior])).toThrow();
    expect(()=>request({...threat,target_object_id:null,target_state_event_id:'threat'},[prior])).toThrow();
  });
  it('distinguishes each autonomy capability and does not imply authorization or consent',()=>{
    expect(autonomyDimensions).toHaveLength(7);
    const decide={...draft,dimension:'AUTONOMY_DECIDE'};
    const refuse={...draft,dimension:'AUTONOMY_REFUSE',judgment:'NOT_INDICATED'};
    expect(request(decide).judgment).toBe('INDICATED');
    expect(request(refuse).judgment).toBe('NOT_INDICATED');
    expect(()=>request({...decide,target_object_id:null,target_actor_id:'b'})).toThrow();
    const prior={event_id:'decide',input:request(decide)};
    expect(()=>request({...refuse,revises_event_id:'decide'},[prior])).toThrow();
  });
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
