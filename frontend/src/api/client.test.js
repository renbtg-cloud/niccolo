import { describe,it,expect } from 'vitest';import { NiccoloClient,artifactsPath,intentPath,operationPath } from './client.js';
const c={workspace:'11111111-1111-4111-8111-111111111111',situation:'22222222-2222-4222-8222-222222222222',token:'x'};
function response(status,value,headers={}){return {status,ok:status>=200&&status<300,headers:{get:(k)=>headers[k.toLowerCase()]??null},json:async()=>value};}
describe('BFF client',()=>{
 it('uses parser-free artifact endpoints for metadata and explicit representations',async()=>{const calls=[];const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o]);return response(o.method==='GET'?200:201,{artifacts:[]});});await x.artifacts();await x.recordArtifact('a',{artifact_type:'VIDEO',label:'meeting.mp4'});await x.recordArtifactStorage('a','s',{storage_type:'S3',locator:'s3://bucket/key'});await x.recordArtifactRepresentation('a','r',{representation_type:'HUMAN_DESCRIPTION',content:'reported text'});await x.recordArtifactRelationship('rel',{subject_artifact_id:'a',object_artifact_id:'b'});expect(artifactsPath(c)).toContain('/situations/22222222-2222-4222-8222-222222222222/artifacts');expect(calls.map(([u])=>u)).toEqual([artifactsPath(c),`${artifactsPath(c)}/a`,`${artifactsPath(c)}/a/storage-references/s`,`${artifactsPath(c)}/a/representations/r`,`/v1/workspaces/${c.workspace}/situations/${c.situation}/artifact-relationships/rel`]);expect(calls.every(([,o])=>o.body===undefined||typeof o.body==='string')).toBe(true);});
 it('records a normative judgment without a permission or execution endpoint',async()=>{const calls=[];const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o]);return response(201,{assessment:{event_id:'event'}});});const body={dimension:'PRECEDENCE',judgment:'CONTESTED',norm_object_id:'norm',competing_norm_object_id:'other'};await x.recordNormativeAssessment('event',body);expect(calls[0][0]).toContain('/epistemic/normative-assessments/event');expect(calls[0][1].method).toBe('POST');expect(JSON.parse(calls[0][1].body)).toEqual(body);});
 it('uses distinct sync/async surfaces over same operation path',async()=>{const calls=[];const f=async(u,o)=>{calls.push([u,o]);return response(o.method==='POST'?202:200,{state:'QUEUED'},{'etag':'"op-1"','x-poll-after-ms':'750'});};const x=new NiccoloClient(c,f);await x.runAsync('33333333-3333-4333-8333-333333333333',{kind:'AGENTIC_ANALYSIS'});expect(calls[0][0]).toContain('/async');expect(operationPath(c,'x')).toContain('/bff/operations/x');});
 it('exposes human intent sync and async surfaces without leaking operation kind',async()=>{const calls=[];const f=async(u,o)=>{calls.push([u,o]);return response(o.method==='POST'?202:200,{analysis_id:'a',intent:'UNDERSTAND',operation:{state:'QUEUED'}});};const x=new NiccoloClient(c,f);const task={analysis_id:'a',intent:'UNDERSTAND',goal:null,context:{},step_budget:32};await x.runIntentSync('op',task);await x.runIntentAsync('op2',task);expect(calls[0][0]).toContain('/bff/intents/op/sync');expect(calls[1][0]).toContain('/bff/intents/op2/async');expect(intentPath(c,'x')).toContain('/bff/intents/x');});
 it('reads the durable human-facing analysis output from the analysis surface',async()=>{const calls=[];const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o.method]);return response(200,{answer_text:'Provisional answer',coverage_note:'Frozen input only',input_is_current:true,historical_output_preserved:true});});const r=await x.analysisOutput('aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa');expect(calls[0][0]).toContain('/analysis-snapshots/aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa/output');expect(calls[0][1]).toBe('GET');expect(r.value.answer_text).toBe('Provisional answer');});
 it('reads frozen context pages and passes through the request cancellation signal',async()=>{
   const calls=[];
   const signal=new AbortController().signal;
   const client=new NiccoloClient(c,async(url,options)=>{
     calls.push({url,signal:options.signal});
     return response(200,{contributions:[],next_after:null});
   });
   await client.analysisContext('analysis',signal);
   await client.analysisContext('analysis',signal,'next-id');
   expect(calls.map(call=>call.url.split('/').at(-1))).toEqual(['context-usage','context-usage?after=next-id']);
   expect(calls.every(call=>call.signal===signal)).toBe(true);
 });
 it('includes the active operation when requesting the review dashboard',async()=>{const calls=[];const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o.method]);return response(200,{analysis:{analysis_id:'a'},operation:{operation_id:'op'}});});await x.dashboard('a','op');expect(calls[0][0]).toContain('/analysis-snapshots/a/bff/dashboard?operation=op');expect(calls[0][1]).toBe('GET');});
 it('preserves conditional pull 304',async()=>{const x=new NiccoloClient(c,async()=>response(304,{}, {'etag':'"same"'}));const r=await x.operation('id','"same"');expect(r.notModified).toBe(true);expect(r.etag).toBe('"same"');});
 it('exposes bounded long-pull and agentic inspection without changing task semantics',async()=>{const calls=[];const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o.method]);return response(200,{state:'RUNNING'});});await x.waitOperation('id',3,2500);await x.agentic('id');await x.planAgentic('id');await x.respondAgentic('id','step',{response_id:'r',decision:'CONTINUE',response_text:'ok'});expect(calls[0][0]).toContain('/wait?after_version=3&timeout_ms=2500');expect(calls[1][0]).toContain('/agentic');expect(calls[2][1]).toBe('POST');expect(calls[3][0]).toContain('/agentic/steps/step/human-response');});
 it('exposes one compact pull observation surface',async()=>{const calls=[];const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o.method]);return response(200,{operation:{state:'RUNNING',state_version:4},events:[],next_after_event:9});});const r=await x.observeOperation('id',4,9,3200,77);expect(calls[0][0]).toContain('/observe?after_version=4&after_event=9&timeout_ms=3200&limit=77');expect(r.value.next_after_event).toBe(9);});

 it('reads the backend-owned epistemic reasoning view',async()=>{const calls=[];const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o.method]);return response(200,{claims:[],claims_are_verified_facts:false,browser_is_domain_authority:false});});const r=await x.epistemic('aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa');expect(calls[0][0]).toContain('/analysis-snapshots/aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa/bff/epistemic');expect(calls[0][1]).toBe('GET');expect(r.value.claims_are_verified_facts).toBe(false);});

 it('lists the workspace source catalog through the BFF client',async()=>{
   const calls=[];
   const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o.method]);return response(200,{documents:[],next_after:null});});
   await x.sources();
   expect(calls[0][0]).toContain('/sources?limit=100');
   await x.sources('77777777-7777-4777-8777-777777777777',25);
   expect(calls[1][0]).toContain('/sources?after=77777777-7777-4777-8777-777777777777&limit=25');
   expect(calls[0][1]).toBe('GET');
 });

 it('covers the explicit human identity mapping surfaces',async()=>{
   const calls=[];
   const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o]);return response(o.method==='POST'?201:200,{identities:[],speakers:[],bindings:[],events:[]});});
   await x.sourceSpeakers('doc');
   await x.identities(null,25);
   await x.createIdentity('person','Ana P.');
   await x.effectiveIdentities('doc');
   await x.identityHistory('doc',3,20);
   await x.recordIdentityEvent('event',{document_id:'doc',speaker_label:'Ana',operation:'assign',person_id:'person',human_reason:'confirmed by user'});
   expect(calls[0][0]).toContain('/sources/doc/speakers');
   expect(calls[1][0]).toContain('/identities?limit=25');
   expect(calls[2][0]).toContain('/identities/person');
   expect(JSON.parse(calls[2][1].body).display_label).toBe('Ana P.');
   expect(calls[3][0]).toContain('/documents/doc/identities');
   expect(calls[4][0]).toContain('/documents/doc/identity-history?after=3&limit=20');
   expect(calls[5][0]).toContain('/identity-events/event');
   expect(JSON.parse(calls[5][1].body).human_reason).toBe('confirmed by user');
 });

 it('submits explicit human-approved reanalysis without rewriting the prior analysis',async()=>{const calls=[];const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o]);return response(200,{kind:'REANALYZE',state:'SUCCEEDED',result:{prior_analysis_id:'old',new_analysis_id:'new',historical_output_preserved:true,human_requested_operation:true}});});const r=await x.runReanalysisSync('op-re','old','new','Revisit with new evidence');expect(calls[0][0]).toContain('/bff/operations/op-re/sync');const body=JSON.parse(calls[0][1].body);expect(body.kind).toBe('REANALYZE');expect(body.target_analysis_id).toBe('old');expect(body.input).toEqual({new_analysis_id:'new',goal:'Revisit with new evidence'});expect(r.value.result.historical_output_preserved).toBe(true);});

 it('creates corrects and retracts attributed human context without calling it evidence',async()=>{const calls=[];const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o]);return response(200,{revision:4,contributions:[]});});await x.addSituationContribution('c1','user_reported','I remember the tone',null);await x.correctSituationContribution('c1','c2','I now remember it differently',null);await x.retractSituationContribution('c2');expect(calls[0][0]).toContain('/contributions/c1');expect(JSON.parse(calls[0][1].body)).toEqual({kind:'user_reported',text:'I remember the tone',related_evidence_id:null});expect(calls[1][0]).toContain('/contributions/c1/corrections/c2');expect(calls[2][0]).toContain('/contributions/c2/retract');});

 it('lists analysis history with preserved lineage and historical output metadata',async()=>{const calls=[];const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o.method]);return response(200,{analyses:[{analysis_id:'new',reanalysis_of:'old',input_revision:3,current_situation_revision:4,input_is_current:false,historical_output_preserved:true,answer_text:'prior answer'}],next_before:null});});const r=await x.analysisHistory(null,100);expect(calls[0][0]).toContain('/analysis-history?limit=100');expect(r.value.analyses[0]).toMatchObject({analysis_id:'new',reanalysis_of:'old',input_is_current:false,historical_output_preserved:true});});

 it('reads recorded reanalysis revision explanation without inferring causality',async()=>{const calls=[];const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o.method]);return response(200,{prior_analysis_id:'old',new_analysis_id:'new',retained:1,refined:0,input_change_proves_causal_reason_for_output_change:false,unlinked_claims_are_inferred_change:false});});const r=await x.reanalysisRevision('new');expect(calls[0][0]).toContain('/analysis-snapshots/new/reanalysis-revision');expect(r.value.input_change_proves_causal_reason_for_output_change).toBe(false);});

 it('preserves rejected insufficient-evidence and unknown feedback as distinct requests',async()=>{const calls=[];const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o]);return response(200,{events:[]});});for(const responseName of ['rejected','insufficient_evidence','unknown'])await x.recordQuestionFeedback('q','e-'+responseName,responseName,'human reason');expect(calls.map(([,o])=>JSON.parse(o.body).response)).toEqual(['rejected','insufficient_evidence','unknown']);expect(calls[0][0]).toContain('/questions/q/feedback/e-rejected');});

 it('lists and answers analysis clarifications without promoting answers to evidence',async()=>{const calls=[];const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o]);return response(o.method==='POST'?201:200,o.method==='POST'?{kind:'USER_REPORTED',is_evidence:false,answer_is_in_original_analysis_input:false}:{clarifications:[],answers:[]});});await x.analysisClarifications('a');await x.clarificationAnswers('a','q');const r=await x.answerClarification('a','q','answer','Human answer');expect(calls[0][0]).toContain('/analysis-snapshots/a/clarifications?limit=25');expect(calls[1][0]).toContain('/analysis-snapshots/a/clarifications/q/answers?limit=25');expect(calls[2][0]).toContain('/analysis-snapshots/a/clarifications/q/answers/answer');expect(JSON.parse(calls[2][1].body)).toEqual({text:'Human answer'});expect(r.value.is_evidence).toBe(false);expect(r.value.answer_is_in_original_analysis_input).toBe(false);});

 it('lists source requests and records only the human decision',async()=>{const calls=[];const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o]);return response(o.method==='POST'?201:200,o.method==='POST'?{replayed:false,system_fetched_external_material:false,completion_establishes_evidence:false}:{source_requests:[],human_owns_external_action:true});});const listed=await x.sourceRequests('a');const recorded=await x.recordSourceRequestEvent('a','request','event','COMPLETED','External acquisition finished.');expect(calls[0][0]).toContain('/analysis-snapshots/a/source-requests');expect(calls[0][1].method).toBe('GET');expect(calls[1][0]).toContain('/source-requests/request/events/event');expect(JSON.parse(calls[1][1].body)).toEqual({kind:'COMPLETED',note:'External acquisition finished.'});expect(listed.value.human_owns_external_action).toBe(true);expect(recorded.value.system_fetched_external_material).toBe(false);expect(recorded.value.completion_establishes_evidence).toBe(false);});

 it('reads provider reasoning input observability including explicit truncation',async()=>{const calls=[];const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o.method]);return response(200,{source_fragment_count_total:300,source_fragment_count_included:256,source_packet_truncated:true,reasoning_packet_sha256:'abc'});});const r=await x.reasoningInput('a');expect(calls[0][0]).toContain('/analysis-snapshots/a/bff/reasoning-input');expect(r.value.source_packet_truncated).toBe(true);expect(r.value.source_fragment_count_included).toBe(256);});
 it('reads the deterministic reasoning handoff independently of active operation state',async()=>{const calls=[];const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o.method]);return response(200,{handoff:{version:2,handoff_grants_authority:false},handoff_sha256:'a'.repeat(64),read_model_is_authority:false});});const r=await x.reasoningHandoff('a');expect(calls[0][0]).toContain('/analysis-snapshots/a/bff/handoff');expect(calls[0][1]).toBe('GET');expect(r.value.handoff.version).toBe(2);expect(r.value.read_model_is_authority).toBe(false);});
 it('lists and records SHA-bound handoff receipts without client-supplied recipient',async()=>{const calls=[];const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o]);return response(o.method==='POST'?201:200,o.method==='POST'?{receipt_id:'r',recipient_principal_id:'server-derived',receipt_means_agreement:false,receipt_grants_authority:false}:{receipts:[{kind:'REVIEWED'}],receipt_means_agreement:false,receipt_grants_authority:false});});const list=await x.handoffReceipts('a');await x.recordHandoffReceipt('a','r','a'.repeat(64),'REVIEWED','looked at it');expect(calls[0][0]).toContain('/analysis-snapshots/a/bff/handoff/receipts');expect(calls[0][1].method).toBe('GET');expect(calls[1][0]).toContain('/analysis-snapshots/a/bff/handoff/receipts/r');expect(JSON.parse(calls[1][1].body)).toEqual({handoff_sha256:'a'.repeat(64),kind:'REVIEWED',note:'looked at it'});expect(JSON.parse(calls[1][1].body)).not.toHaveProperty('recipient_principal_id');expect(list.value.receipt_means_agreement).toBe(false);});
});

it('passes the observation cancellation signal to fetch', async () => {
  const controller = new AbortController();
  const client = new NiccoloClient({ workspace: 'w', situation: 's', token: 'fixture' }, async (_, options) => {
    expect(options.signal).toBe(controller.signal);
    return { status: 200, ok: true, json: async () => ({}), headers: new Headers() };
  });
  await client.observeOperation('operation', 0, 0, 5000, 100, controller.signal);
});

it('queries recognizability with separate event and availability dates', async () => {
  const { signal } = new AbortController();
  const urls = [];
  const client = new NiccoloClient(c, async (path, options) => {
    urls.push(new URL(path, 'http://localhost'));
    expect(options.method).toBe('GET');
    expect(options.signal).toBe(signal);
    return response(200, { records: [] });
  });
  const lookup = {
    subject_actor_id: 'frank',
    audience_actor_id: 'evening-class',
    on: '2026-11-19',
    known_through: '2026-11-20',
  };
  await client.publicProminence(lookup, signal);
  await client.publicProminence({ ...lookup, cue: 'FACE' }, signal);
  await client.publicProminence({ ...lookup, on: null }, signal);
  expect(urls[0].pathname).toBe(`/v1/workspaces/${c.workspace}/situations/${c.situation}/epistemic/public-prominence`);
  expect(Object.fromEntries(urls[0].searchParams)).toEqual(lookup);
  expect(Object.fromEntries(urls[1].searchParams)).toEqual({ ...lookup, cue: 'FACE' });
  expect(urls[2].searchParams.has('on')).toBe(false);
  expect(urls[2].searchParams.get('known_through')).toBe(lookup.known_through);
});

it('posts an attributed recognizability report through the Situation client', async () => {
  const input = {
    subject_actor_id: 'frank', audience_actor_id: 'evening-class', cue: 'FACE',
    observation: 'EXPOSURE_REPORTED', audience_scope: 'Mara only',
    effective_from: '2026-11-19', effective_through: '2026-11-19', available_on: '2026-11-20',
    claim_object_id: 'proposition', representation_id: 'passage',
  };
  const client = new NiccoloClient(c, async (path, options) => {
    expect(path).toBe(`/v1/workspaces/${c.workspace}/situations/${c.situation}/epistemic/public-prominence/record`);
    expect(options.method).toBe('POST');
    expect(JSON.parse(options.body)).toEqual(input);
    return response(201, { record_id: 'record' });
  });
  expect((await client.recordPublicProminence('record', input)).value.record_id).toBe('record');
});

it('asks the backend to review only the selected detail and exposure', async () => {
  const body = {
    witness_actor_id: 'mara', exposure_record_id: 'one-exposure',
    testimony_detail: { object_id: 'hat', representation_id: 'interview-detail' },
    candidate_source: { object_id: 'archive-photo', representation_id: 'photo-description' },
    testimony_from: '2026-11-20', testimony_through: '2026-11-20', known_through: '2026-11-21',
  };
  const path = `/v1/workspaces/${c.workspace}/situations/${c.situation}/epistemic/detail-exposure-review`;
  const decision = {
    argument_id: 'arg', grounding_set_id: 'hat-route', disposition: 'SUSPEND',
    rationale: 'Check the archive source first.', review: body, expected_review_sha256: 'a'.repeat(64),
  };
  const client = new NiccoloClient(c, async (url, options) => {
    expect(options.method).toBe('POST');
    if (url === path) {
      expect(JSON.parse(options.body)).toEqual(body);
      return response(200, { disposition: 'REVIEW_CANDIDATE' });
    }
    expect(url).toBe(`${path}/route-use/event`);
    expect(JSON.parse(options.body)).toEqual(decision);
    return response(201, { decision: { event_id: 'event' } });
  });
  expect((await client.reviewDetailExposure(body)).value.disposition).toBe('REVIEW_CANDIDATE');
  expect((await client.recordDetailExposureRouteUse('event', decision)).value.decision.event_id).toBe('event');
});

it('records an attributed order without supplying clock times', async () => {
  const body = {
    exposure_record_id: 'viewing', testimony_representation_id: 'testimony',
    candidate_representation_id: 'photo', direction: 'EXPOSURE_BEFORE_TESTIMONY',
    source: {object_id: 'order-proposition', representation_id: 'follow-up'}, available_on: '2026-11-24',
  };
  const client = new NiccoloClient(c, async (url, options) => {
    expect(url).toBe(`/v1/workspaces/${c.workspace}/situations/${c.situation}/epistemic/detail-exposure-review/order-reports/order`);
    expect(options.method).toBe('POST');
    expect(JSON.parse(options.body)).toEqual(body);
    return response(201, {report_id: 'order'});
  });
  expect((await client.recordExposureOrder('order', body)).value.report_id).toBe('order');
});

it('passes cancellation through a review read and rejects an aborted body', async () => {
  const controller = new AbortController();
  const client = new NiccoloClient({workspace:'w', situation:'s', token:'fixture'}, async (_, options) => {
    expect(options.signal).toBe(controller.signal);
    return {status:200,ok:true,json:async()=>{controller.abort();throw controller.signal.reason;}};
  });
  await expect(client.analysisReview('analysis', controller.signal)).rejects.toMatchObject({name:'AbortError'});
});

it('passes the selected cursor and cancellation signal through feedback reads', async () => {
  const { signal } = new AbortController();
  const paths = [];
  const client = new NiccoloClient({workspace:'w', situation:'s', token:'fixture'}, async (path, options) => {
    expect(options.signal).toBe(signal);
    paths.push(path);
    return response(200, {});
  });
  await client.questionFeedback('q', 40, 100, signal);
  await client.questionPositions('q', 'actor', 100, signal);
  expect(paths).toEqual([
    '/v1/workspaces/w/questions/q/feedback?after=40&limit=100',
    '/v1/workspaces/w/questions/q/positions?after=actor&limit=100',
  ]);
});
