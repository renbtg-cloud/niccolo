import { describe,it,expect } from 'vitest';import { NiccoloClient,intentPath,operationPath } from './client.js';
const c={workspace:'11111111-1111-4111-8111-111111111111',situation:'22222222-2222-4222-8222-222222222222',token:'x'};
function response(status,value,headers={}){return {status,ok:status>=200&&status<300,headers:{get:(k)=>headers[k.toLowerCase()]??null},json:async()=>value};}
describe('BFF client',()=>{
 it('uses distinct sync/async surfaces over same operation path',async()=>{const calls=[];const f=async(u,o)=>{calls.push([u,o]);return response(o.method==='POST'?202:200,{state:'QUEUED'},{'etag':'"op-1"','x-poll-after-ms':'750'});};const x=new NiccoloClient(c,f);await x.runAsync('33333333-3333-4333-8333-333333333333',{kind:'AGENTIC_ANALYSIS'});expect(calls[0][0]).toContain('/async');expect(operationPath(c,'x')).toContain('/bff/operations/x');});
 it('exposes human intent sync and async surfaces without leaking operation kind',async()=>{const calls=[];const f=async(u,o)=>{calls.push([u,o]);return response(o.method==='POST'?202:200,{analysis_id:'a',intent:'UNDERSTAND',operation:{state:'QUEUED'}});};const x=new NiccoloClient(c,f);const task={analysis_id:'a',intent:'UNDERSTAND',goal:null,context:{},step_budget:32};await x.runIntentSync('op',task);await x.runIntentAsync('op2',task);expect(calls[0][0]).toContain('/bff/intents/op/sync');expect(calls[1][0]).toContain('/bff/intents/op2/async');expect(intentPath(c,'x')).toContain('/bff/intents/x');});
 it('reads the durable human-facing analysis output from the analysis surface',async()=>{const calls=[];const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o.method]);return response(200,{answer_text:'Provisional answer',coverage_note:'Frozen input only',input_is_current:true,historical_output_preserved:true});});const r=await x.analysisOutput('aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa');expect(calls[0][0]).toContain('/analysis-snapshots/aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa/output');expect(calls[0][1]).toBe('GET');expect(r.value.answer_text).toBe('Provisional answer');});
 it('includes the active operation when requesting the review dashboard',async()=>{const calls=[];const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o.method]);return response(200,{analysis:{analysis_id:'a'},operation:{operation_id:'op'}});});await x.dashboard('a','op');expect(calls[0][0]).toContain('/analysis-snapshots/a/bff/dashboard?operation=op');expect(calls[0][1]).toBe('GET');});
 it('preserves conditional pull 304',async()=>{const x=new NiccoloClient(c,async()=>response(304,{}, {'etag':'"same"'}));const r=await x.operation('id','"same"');expect(r.notModified).toBe(true);expect(r.etag).toBe('"same"');});
 it('exposes bounded long-pull and agentic inspection without changing task semantics',async()=>{const calls=[];const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o.method]);return response(200,{state:'RUNNING'});});await x.waitOperation('id',3,2500);await x.agentic('id');await x.planAgentic('id');await x.respondAgentic('id','step',{response_id:'r',decision:'CONTINUE',response_text:'ok'});expect(calls[0][0]).toContain('/wait?after_version=3&timeout_ms=2500');expect(calls[1][0]).toContain('/agentic');expect(calls[2][1]).toBe('POST');expect(calls[3][0]).toContain('/agentic/steps/step/human-response');});
 it('exposes one compact pull observation surface',async()=>{const calls=[];const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o.method]);return response(200,{operation:{state:'RUNNING',state_version:4},events:[],next_after_event:9});});const r=await x.observeOperation('id',4,9,3200,77);expect(calls[0][0]).toContain('/observe?after_version=4&after_event=9&timeout_ms=3200&limit=77');expect(r.value.next_after_event).toBe(9);});

 it('reads the backend-owned epistemic reasoning view',async()=>{const calls=[];const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o.method]);return response(200,{claims:[],claims_are_verified_facts:false,browser_is_domain_authority:false});});const r=await x.epistemic('aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa');expect(calls[0][0]).toContain('/analysis-snapshots/aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa/bff/epistemic');expect(calls[0][1]).toBe('GET');expect(r.value.claims_are_verified_facts).toBe(false);});

 it('uploads raw source bytes and attaches the document to the Situation',async()=>{
   const calls=[];
   const x=new NiccoloClient(c,async(u,o)=>{
     calls.push([u,o]);
     return response(o.method==='POST'?201:200,{document_id:'doc',source_document_ids:['doc']});
   });
   const raw=new Uint8Array([65,66,67]);
   await x.ingestSource('doc',{locator:'chat.txt',contentType:'text/plain',body:raw});
   await x.addSituationSource('doc');
   expect(calls[0][0]).toContain('/v1/workspaces/11111111-1111-4111-8111-111111111111/sources/doc');
   expect(calls[0][1].headers['Content-Type']).toBe('text/plain');
   expect(calls[0][1].headers['X-Source-Locator']).toBe('chat.txt');
   expect(calls[0][1].body).toBe(raw);
   expect(calls[1][0]).toContain('/situations/22222222-2222-4222-8222-222222222222/sources/doc');
   expect(calls[1][1].method).toBe('POST');
 });
 it('lists the workspace source catalog through the BFF client',async()=>{
   const calls=[];
   const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o.method]);return response(200,{documents:[],next_after:null});});
   await x.sources(7,25);
   expect(calls[0][0]).toContain('/sources?after=7&limit=25');
   expect(calls[0][1]).toBe('GET');
 });

 it('covers the explicit human identity mapping surfaces',async()=>{
   const calls=[];
   const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o]);return response(o.method==='POST'?201:200,{identities:[],speakers:[],bindings:[],events:[]});});
   await x.sourceSpeakers('doc');
   await x.identities(0,25);
   await x.createIdentity('person','Ana P.');
   await x.effectiveIdentities('doc');
   await x.identityHistory('doc',3,20);
   await x.recordIdentityEvent('event',{document_id:'doc',speaker_label:'Ana',operation:'assign',person_id:'person',human_reason:'confirmed by user'});
   expect(calls[0][0]).toContain('/sources/doc/speakers');
   expect(calls[1][0]).toContain('/identities?after=0&limit=25');
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

 it('reads provider reasoning input observability including explicit truncation',async()=>{const calls=[];const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o.method]);return response(200,{source_fragment_count_total:300,source_fragment_count_included:256,source_packet_truncated:true,reasoning_packet_sha256:'abc'});});const r=await x.reasoningInput('a');expect(calls[0][0]).toContain('/analysis-snapshots/a/bff/reasoning-input');expect(r.value.source_packet_truncated).toBe(true);expect(r.value.source_fragment_count_included).toBe(256);});
});
