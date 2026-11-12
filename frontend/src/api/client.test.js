import { describe,it,expect } from 'vitest';import { NiccoloClient,intentPath,operationPath } from './client.js';
const c={workspace:'11111111-1111-4111-8111-111111111111',situation:'22222222-2222-4222-8222-222222222222',token:'x'};
function response(status,value,headers={}){return {status,ok:status>=200&&status<300,headers:{get:(k)=>headers[k.toLowerCase()]??null},json:async()=>value};}
describe('BFF client',()=>{
 it('reads bounded GitHub publication event history without provider authorization',async()=>{const calls=[];const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o]);return response(200,{events:[],provider_secret_stored:false});});const r=await x.gitPublicationRemoteEvents('analysis','publication',17);expect(calls[0][0]).toContain('/git-publication-intents/publication/remote-events?limit=17');expect(calls[0][1].method).toBe('GET');expect(calls[0][1].headers['X-Niccolo-GitHub-Authorization']).toBeUndefined();expect(r.value.provider_secret_stored).toBe(false);});
 it('verifies a GitHub publication target with transient authorization only',async()=>{const calls=[];const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o]);return response(200,{repository_full_name:'owner/repo',remote_changes_performed:false,provider_secret_stored:false});});const r=await x.verifyGitPublicationTarget('target','transient-token');expect(calls[0][0]).toContain('/git-publication-targets/target/verify');expect(calls[0][1].method).toBe('POST');expect(calls[0][1].headers['X-Niccolo-GitHub-Authorization']).toBe('Bearer transient-token');expect(r.value.remote_changes_performed).toBe(false);});
 it('uses distinct sync/async surfaces over same operation path',async()=>{const calls=[];const f=async(u,o)=>{calls.push([u,o]);return response(o.method==='POST'?202:200,{state:'QUEUED'},{'etag':'"op-1"','x-poll-after-ms':'750'});};const x=new NiccoloClient(c,f);await x.runAsync('33333333-3333-4333-8333-333333333333',{kind:'AGENTIC_ANALYSIS'});expect(calls[0][0]).toContain('/async');expect(operationPath(c,'x')).toContain('/bff/operations/x');});
 it('exposes human intent sync and async surfaces without leaking operation kind',async()=>{const calls=[];const f=async(u,o)=>{calls.push([u,o]);return response(o.method==='POST'?202:200,{analysis_id:'a',intent:'UNDERSTAND',operation:{state:'QUEUED'}});};const x=new NiccoloClient(c,f);const task={analysis_id:'a',intent:'UNDERSTAND',goal:null,context:{},step_budget:32};await x.runIntentSync('op',task);await x.runIntentAsync('op2',task);expect(calls[0][0]).toContain('/bff/intents/op/sync');expect(calls[1][0]).toContain('/bff/intents/op2/async');expect(intentPath(c,'x')).toContain('/bff/intents/x');});
 it('reads the durable human-facing analysis output from the analysis surface',async()=>{const calls=[];const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o.method]);return response(200,{answer_text:'Provisional answer',coverage_note:'Frozen input only',input_is_current:true,historical_output_preserved:true});});const r=await x.analysisOutput('aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa');expect(calls[0][0]).toContain('/analysis-snapshots/aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa/output');expect(calls[0][1]).toBe('GET');expect(r.value.answer_text).toBe('Provisional answer');});
 it('includes the active operation when requesting the review dashboard',async()=>{const calls=[];const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o.method]);return response(200,{analysis:{analysis_id:'a'},operation:{operation_id:'op'}});});await x.dashboard('a','op');expect(calls[0][0]).toContain('/analysis-snapshots/a/bff/dashboard?operation=op');expect(calls[0][1]).toBe('GET');});
 it('preserves conditional pull 304',async()=>{const x=new NiccoloClient(c,async()=>response(304,{}, {'etag':'"same"'}));const r=await x.operation('id','"same"');expect(r.notModified).toBe(true);expect(r.etag).toBe('"same"');});
 it('exposes bounded long-pull and agentic inspection without changing task semantics',async()=>{const calls=[];const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o.method]);return response(200,{state:'RUNNING'});});await x.waitOperation('id',3,2500);await x.agentic('id');await x.planAgentic('id');await x.respondAgentic('id','step',{response_id:'r',decision:'CONTINUE',response_text:'ok'});expect(calls[0][0]).toContain('/wait?after_version=3&timeout_ms=2500');expect(calls[1][0]).toContain('/agentic');expect(calls[2][1]).toBe('POST');expect(calls[3][0]).toContain('/agentic/steps/step/human-response');});
 it('reads bounded anytime checkpoints without changing operation state',async()=>{const calls=[];const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o.method]);return response(200,{checkpoints:[{checkpoint_kind:'STEP_COMPLETED',provisional:true}],next_after:null});});const r=await x.operationCheckpoints('op',7,33);expect(calls[0][0]).toContain('/bff/operations/op/checkpoints?after=7&limit=33');expect(calls[0][1]).toBe('GET');expect(r.value.checkpoints[0].provisional).toBe(true);});
 it('downloads one bounded checkpoint package without changing operation state',async()=>{const calls=[];const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o.method]);return response(200,{format:'niccolo.operation-checkpoint-package',published_to_git:false,files:{'checkpoint.json':'{}'}});});const r=await x.operationCheckpointPackage('op','cp');expect(calls[0][0]).toContain('/bff/operations/op/checkpoints/cp/package');expect(calls[0][1]).toBe('GET');expect(r.value.published_to_git).toBe(false);});

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

 it('exposes forensic export import quarantine inspection and explicit analysis binding',async()=>{
   const calls=[];
   const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o]);return response(o.method==='POST'?201:200,{batches:[],items:[]});});
   await x.forensicExport();
   await x.forensicImports(null,25);
   await x.forensicImports('batch-cursor',10);
   await x.forensicImport('batch');
   await x.importForensic('batch','file://prior-niccolo.json',{format:'NICCOLO_SITUATION_FORENSIC_EXPORT'});
   await x.forensicImportItems('batch','logical/tables/a row.json',20);
   await x.forensicImportItem('batch','item');
   const rawBlob={size:3,type:'application/octet-stream'};
   const downloadClient=new NiccoloClient(c,async(u,o)=>{calls.push([u,o]);return {status:200,ok:true,headers:{get:()=>null},json:async()=>({}),blob:async()=>rawBlob};});
   expect(await downloadClient.downloadForensicOriginal('batch','item')).toBe(rawBlob);
   await x.analysisForensicContext('analysis');
   await x.bindAnalysisForensicContext('analysis','binding','batch','item','Use only as historical context');
   expect(calls[0][0]).toContain('/situations/22222222-2222-4222-8222-222222222222/forensic-export');
   expect(calls[1][0]).toContain('/forensic-imports?limit=25');
   expect(calls[2][0]).toContain('/forensic-imports?before=batch-cursor&limit=10');
   expect(calls[3][0]).toContain('/forensic-imports/batch');
   expect(JSON.parse(calls[4][1].body)).toEqual({declared_origin:'file://prior-niccolo.json',package:{format:'NICCOLO_SITUATION_FORENSIC_EXPORT'}});
   expect(calls[5][0]).toContain('/forensic-imports/batch/items?after=logical%2Ftables%2Fa%20row.json&limit=20');
   expect(calls[6][0]).toContain('/forensic-imports/batch/items/item');
   expect(calls[7][0]).toContain('/forensic-imports/batch/items/item/original');
   expect(calls[7][1].headers.Accept).toBe('application/octet-stream');
   expect(calls[8][0]).toContain('/analysis-snapshots/analysis/forensic-context');
   expect(JSON.parse(calls[9][1].body)).toEqual({import_batch_id:'batch',item_id:'item',note:'Use only as historical context'});
 });

 it('keeps Git publication planning local and explicit at the client boundary',async()=>{
   const calls=[]; const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o]);return response(o.method==='POST'?201:200,{targets:[],intents:[],package_sha256:'a'.repeat(64)});});
   await x.gitPublicationTargets();
   await x.registerGitPublicationTarget('target',{repository_owner:'owner',repository_name:'history',ref_name:'main',repository_visibility:'PRIVATE',sensitive_exports_allowed:false});
   await x.gitPublicationPreview('analysis','target');
   await x.gitPublicationIntents('analysis',12);
   await x.recordGitPublicationIntent('analysis','publication','target');
   expect(calls[0][0]).toContain('/git-publication-targets');
   expect(calls[1][0]).toContain('/git-publication-targets/target');
   expect(calls[2][0]).toContain('/analysis-snapshots/analysis/git-publication-preview?target_id=target');
   expect(calls[3][0]).toContain('/analysis-snapshots/analysis/git-publication-intents?limit=12');
   expect(calls[4][0]).toContain('/analysis-snapshots/analysis/git-publication-intents/publication');
   expect(JSON.parse(calls[4][1].body)).toEqual({target_id:'target'});
 });

 it('sends transient GitHub authorization only in the dedicated publish request header',async()=>{
   const calls=[];
   const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o]);return response(200,{remote_publish_verified:true,status:{state:'VERIFIED_PUBLISHED'}});});
   await x.gitPublicationRemoteStatus('analysis','publication');
   await x.publishGitPublicationIntent('analysis','publication','attempt','ghp_transient_only');
   expect(calls[0][0]).toContain('/git-publication-intents/publication/remote-status');
   expect(calls[0][1].headers['X-Niccolo-GitHub-Authorization']).toBeUndefined();
   expect(calls[1][0]).toContain('/git-publication-intents/publication/publish');
   expect(calls[1][0]).not.toContain('ghp_transient_only');
   expect(calls[1][1].headers['X-Niccolo-GitHub-Authorization']).toBe('Bearer ghp_transient_only');
   expect(JSON.parse(calls[1][1].body)).toEqual({attempt_id:'attempt',confirmation:'PUBLISH_STANDARD_PRESENTATION_TO_GITHUB'});
   expect(calls[1][1].cache).toBe('no-store');
 });

 it('keeps explicit milestone declaration separate from checkpoint epistemic and remote-publication state',async()=>{
   const calls=[];
   const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o]);return response(200,{milestone:{checkpoint_provisional:true,remote_publish_performed:false}});});
   await x.operationCheckpointMilestone('operation','checkpoint');
   await x.declareOperationCheckpointMilestone('operation','checkpoint','milestone','TIMED_PARTIAL','Preserve bounded partial result for review');
   expect(calls[0][0]).toContain('/operations/operation/checkpoints/checkpoint/milestone');
   expect(calls[0][1].method).toBe('GET');
   expect(calls[1][0]).toContain('/checkpoints/checkpoint/milestones/milestone');
   expect(calls[1][1].method).toBe('POST');
   expect(JSON.parse(calls[1][1].body)).toEqual({milestone_kind:'TIMED_PARTIAL',reason:'Preserve bounded partial result for review'});
   expect(calls[1][0]).not.toContain('Preserve bounded partial result for review');
 });

 it('downloads an exact milestone package only when a milestone id is selected',async()=>{
   const calls=[];
   const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o]);return response(200,{format:'niccolo.operation-checkpoint-package',scope_kind:'OPERATION_MILESTONE'});});
   await x.operationCheckpointPackage('operation','checkpoint','milestone id');
   expect(calls[0][0]).toContain('/checkpoints/checkpoint/package?milestone_id=milestone%20id');
 });

 it('binds meaningful milestone publication to the exact declared milestone without putting its reason in the URL',async()=>{
   const calls=[];
   const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o]);return response(200,{scope_kind:'OPERATION_MILESTONE',milestone_id:'milestone'});});
   await x.operationCheckpointGitPublicationPreview('operation','checkpoint','target','milestone');
   await x.recordOperationCheckpointGitPublicationIntent('operation','checkpoint','publication','target','milestone');
   expect(calls[0][0]).toContain('/checkpoints/checkpoint/git-publication-preview?target_id=target&milestone_id=milestone');
   expect(calls[1][0]).toContain('/git-publication-intents/publication');
   expect(JSON.parse(calls[1][1].body)).toEqual({target_id:'target',milestone_id:'milestone'});
 });

 it('binds checkpoint publication to exact checkpoint scope and keeps provider authorization transient',async()=>{
   const calls=[];
   const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o]);return response(200,{remote_publish_verified:true,status:{state:'VERIFIED_PUBLISHED'}});});
   await x.operationCheckpointGitPublicationPreview('operation','checkpoint','target');
   await x.recordOperationCheckpointGitPublicationIntent('operation','checkpoint','publication','target');
   await x.operationCheckpointGitPublicationRemoteStatus('operation','checkpoint','publication');
   await x.operationCheckpointGitPublicationRemoteEvents('operation','checkpoint','publication',7);
   await x.publishOperationCheckpointGitPublicationIntent('operation','checkpoint','publication','attempt','checkpoint-transient-token');
   expect(calls[0][0]).toContain('/operations/operation/checkpoints/checkpoint/git-publication-preview?target_id=target');
   expect(calls[1][0]).toContain('/checkpoints/checkpoint/git-publication-intents/publication');
   expect(JSON.parse(calls[1][1].body)).toEqual({target_id:'target'});
   expect(calls[2][0]).toContain('/git-publication-intents/publication/remote-status');
   expect(calls[2][1].headers['X-Niccolo-GitHub-Authorization']).toBeUndefined();
   expect(calls[3][0]).toContain('/git-publication-intents/publication/remote-events?limit=7');
   expect(calls[4][0]).toContain('/git-publication-intents/publication/publish');
   expect(calls[4][0]).not.toContain('checkpoint-transient-token');
   expect(calls[4][1].headers['X-Niccolo-GitHub-Authorization']).toBe('Bearer checkpoint-transient-token');
   expect(JSON.parse(calls[4][1].body)).toEqual({attempt_id:'attempt',confirmation:'PUBLISH_OPERATION_CHECKPOINT_TO_GITHUB'});
   expect(calls[4][1].cache).toBe('no-store');
 });

 it('separates standard analysis export from explicitly authorized examined-source export',async()=>{
   const calls=[];const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o]);return response(200,{format:'niccolo.presentation-package',files:{'README.md':'x'},published_to_git:false});});
   await x.presentationPackage('a');
   await x.presentationSourceExportPreview('a');
   await x.presentationExportEvents('a',12);
   await x.presentationPackageWithExaminedSources('a','event-1');
   expect(calls[0][0]).toContain('/analysis-snapshots/a/presentation-package');
   expect(calls[0][1].method).toBe('GET');
   expect(calls[1][0]).toContain('/analysis-snapshots/a/presentation-package/source-preview');
   expect(calls[1][1].method).toBe('GET');
   expect(calls[2][0]).toContain('/analysis-snapshots/a/presentation-package/export-events?limit=12');
   expect(calls[2][1].method).toBe('GET');
   expect(calls[3][0]).toContain('/analysis-snapshots/a/presentation-package');
   expect(calls[3][1].method).toBe('POST');
   expect(JSON.parse(calls[3][1].body)).toEqual({include_examined_source_fragments:true,export_event_id:'event-1'});
 });

 it('submits explicit human-approved reanalysis without rewriting the prior analysis',async()=>{const calls=[];const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o]);return response(200,{kind:'REANALYZE',state:'SUCCEEDED',result:{prior_analysis_id:'old',new_analysis_id:'new',historical_output_preserved:true,human_requested_operation:true}});});const r=await x.runReanalysisSync('op-re','old','new','Revisit with new evidence');expect(calls[0][0]).toContain('/bff/operations/op-re/sync');const body=JSON.parse(calls[0][1].body);expect(body.kind).toBe('REANALYZE');expect(body.target_analysis_id).toBe('old');expect(body.input).toEqual({new_analysis_id:'new',goal:'Revisit with new evidence'});expect(r.value.result.historical_output_preserved).toBe(true);});

 it('creates corrects and retracts attributed human context without calling it evidence',async()=>{const calls=[];const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o]);return response(200,{revision:4,contributions:[]});});await x.addSituationContribution('c1','user_reported','I remember the tone',null);await x.correctSituationContribution('c1','c2','I now remember it differently',null);await x.retractSituationContribution('c2');expect(calls[0][0]).toContain('/contributions/c1');expect(JSON.parse(calls[0][1].body)).toEqual({kind:'user_reported',text:'I remember the tone',related_evidence_id:null});expect(calls[1][0]).toContain('/contributions/c1/corrections/c2');expect(calls[2][0]).toContain('/contributions/c2/retract');});

 it('lists analysis history with preserved lineage and historical output metadata',async()=>{const calls=[];const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o.method]);return response(200,{analyses:[{analysis_id:'new',reanalysis_of:'old',input_revision:3,current_situation_revision:4,input_is_current:false,historical_output_preserved:true,answer_text:'prior answer'}],next_before:null});});const r=await x.analysisHistory(null,100);expect(calls[0][0]).toContain('/analysis-history?limit=100');expect(r.value.analyses[0]).toMatchObject({analysis_id:'new',reanalysis_of:'old',input_is_current:false,historical_output_preserved:true});});

 it('reads recorded reanalysis revision explanation without inferring causality',async()=>{const calls=[];const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o.method]);return response(200,{prior_analysis_id:'old',new_analysis_id:'new',retained:1,refined:0,input_change_proves_causal_reason_for_output_change:false,unlinked_claims_are_inferred_change:false});});const r=await x.reanalysisRevision('new');expect(calls[0][0]).toContain('/analysis-snapshots/new/reanalysis-revision');expect(r.value.input_change_proves_causal_reason_for_output_change).toBe(false);});

 it('preserves rejected insufficient-evidence and unknown feedback as distinct requests',async()=>{const calls=[];const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o]);return response(200,{events:[]});});for(const responseName of ['rejected','insufficient_evidence','unknown'])await x.recordQuestionFeedback('q','e-'+responseName,responseName,'human reason');expect(calls.map(([,o])=>JSON.parse(o.body).response)).toEqual(['rejected','insufficient_evidence','unknown']);expect(calls[0][0]).toContain('/questions/q/feedback/e-rejected');});

 it('lists and answers analysis clarifications without promoting answers to evidence',async()=>{const calls=[];const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o]);return response(o.method==='POST'?201:200,o.method==='POST'?{kind:'USER_REPORTED',is_evidence:false,answer_is_in_original_analysis_input:false}:{clarifications:[],answers:[]});});await x.analysisClarifications('a');await x.clarificationAnswers('a','q');const r=await x.answerClarification('a','q','answer','Human answer');expect(calls[0][0]).toContain('/analysis-snapshots/a/clarifications?limit=25');expect(calls[1][0]).toContain('/analysis-snapshots/a/clarifications/q/answers?limit=25');expect(calls[2][0]).toContain('/analysis-snapshots/a/clarifications/q/answers/answer');expect(JSON.parse(calls[2][1].body)).toEqual({text:'Human answer'});expect(r.value.is_evidence).toBe(false);expect(r.value.answer_is_in_original_analysis_input).toBe(false);});

 it('reads provider reasoning input observability including explicit truncation',async()=>{const calls=[];const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o.method]);return response(200,{source_fragment_count_total:300,source_fragment_count_included:256,source_packet_truncated:true,reasoning_packet_sha256:'abc'});});const r=await x.reasoningInput('a');expect(calls[0][0]).toContain('/analysis-snapshots/a/bff/reasoning-input');expect(r.value.source_packet_truncated).toBe(true);expect(r.value.source_fragment_count_included).toBe(256);});
 it('reads the deterministic reasoning handoff independently of active operation state',async()=>{const calls=[];const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o.method]);return response(200,{handoff:{version:2,handoff_grants_authority:false},handoff_sha256:'a'.repeat(64),read_model_is_authority:false});});const r=await x.reasoningHandoff('a');expect(calls[0][0]).toContain('/analysis-snapshots/a/bff/handoff');expect(calls[0][1]).toBe('GET');expect(r.value.handoff.version).toBe(2);expect(r.value.read_model_is_authority).toBe(false);});
 it('lists and records SHA-bound handoff receipts without client-supplied recipient',async()=>{const calls=[];const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o]);return response(o.method==='POST'?201:200,o.method==='POST'?{receipt_id:'r',recipient_principal_id:'server-derived',receipt_means_agreement:false,receipt_grants_authority:false}:{receipts:[{kind:'REVIEWED'}],receipt_means_agreement:false,receipt_grants_authority:false});});const list=await x.handoffReceipts('a');await x.recordHandoffReceipt('a','r','a'.repeat(64),'REVIEWED','looked at it');expect(calls[0][0]).toContain('/analysis-snapshots/a/bff/handoff/receipts');expect(calls[0][1].method).toBe('GET');expect(calls[1][0]).toContain('/analysis-snapshots/a/bff/handoff/receipts/r');expect(JSON.parse(calls[1][1].body)).toEqual({handoff_sha256:'a'.repeat(64),kind:'REVIEWED',note:'looked at it'});expect(JSON.parse(calls[1][1].body)).not.toHaveProperty('recipient_principal_id');expect(list.value.receipt_means_agreement).toBe(false);});
 it('separates live grounded evaluation from immutable score recording',async()=>{
   const calls=[];
   const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o]);return response(o.method==='POST'?200:200,o.method==='POST'?{assessment:{report:{net_points:7}}}:{records:[]});});
   const body={human_declared_confidence_percent:null,evidence:[{evidence_id:'e',declared_strength:50,rationale:'human rationale'}]};
   await x.groundedScoreRecords('analysis',77);
   await x.evaluateGroundedScore('analysis','claim',body);
   await x.recordGroundedScore('analysis','claim','score',body);
   expect(calls[0][0]).toContain('/analysis-snapshots/analysis/epistemic/records?limit=77');
   expect(calls[1][0]).toContain('/analysis-snapshots/analysis/claims/claim/epistemic/evaluate');
   expect(calls[2][0]).toContain('/analysis-snapshots/analysis/claims/claim/epistemic/records/score');
   expect(JSON.parse(calls[1][1].body)).toEqual(body);
   expect(JSON.parse(calls[2][1].body)).toEqual(body);
 });

});

describe('original source downloads', () => {
  it('downloads exact binary bytes with session authorization', async () => {
    const source = new Blob([new Uint8Array([0, 255, 10, 65])]);
    const calls = [];
    const client = new NiccoloClient(c, async (url, options) => {
      calls.push([url, options]);
      return { ok: true, status: 200, blob: async () => source };
    });
    const result = await client.downloadSourceOriginal('doc');
    expect(new Uint8Array(await result.arrayBuffer())).toEqual(new Uint8Array([0, 255, 10, 65]));
    expect(calls[0][0]).toContain('/sources/doc/original');
    expect(calls[0][1].headers.Authorization).toBe('Bearer x');
    expect(calls[0][1].cache).toBe('no-store');
  });

  it('does not present missing legacy source bytes as a successful download', async () => {
    const client = new NiccoloClient(c, async () => ({
      ok: false, status: 404,
      json: async () => ({ error: 'source original unavailable' }),
    }));
    await expect(client.downloadSourceOriginal('legacy')).rejects.toThrow('source original unavailable');
  });
 it('reads aggregate workspace integration diagnostics without analysis state',async()=>{
   const calls=[];
   const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o.method]);return response(200,{warning_codes:['DUPLICATE_OPERATIONAL_DELIVERIES'],interpretation:'transport only'});});
   const r=await x.integrationDiagnostics();
   expect(calls[0][0]).toContain('/v1/workspaces/11111111-1111-4111-8111-111111111111/integration-diagnostics');
   expect(calls[0][1]).toBe('GET');
   expect(r.value.warning_codes).toEqual(['DUPLICATE_OPERATIONAL_DELIVERIES']);
 });

 it('records and lists attributed upstream processing declarations',async()=>{
   const calls=[];
   const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o]);return response(o.method==='POST'?201:200,o.method==='POST'?{declaration_is_verified:false,declaration_is_evidence:false}:{declarations:[],declarations_are_verified:false,declarations_are_evidence:false});});
   await x.integrationProcessingDeclarations(7,25);
   await x.recordIntegrationProcessingDeclaration('decl',{processing_kind:'FILTER',scope_locator:'s3://mail',declaration_text:'Integrator reports a date filter.',source_document_id:null});
   expect(calls[0][0]).toContain('/integration-processing-declarations?after=7&limit=25');
   expect(calls[0][1].method).toBe('GET');
   expect(calls[1][0]).toContain('/integration-processing-declarations/decl');
   expect(calls[1][1].method).toBe('POST');
   expect(JSON.parse(calls[1][1].body)).toEqual({processing_kind:'FILTER',scope_locator:'s3://mail',declaration_text:'Integrator reports a date filter.',source_document_id:null});
 });

 it('reads authenticated runtime resource diagnostics',async()=>{
   const calls=[];
   const x=new NiccoloClient(c,async(u,o)=>{calls.push([u,o.method]);return response(200,{advice_is_operational_only:true,establishes_epistemic_quality:false});});
   const r=await x.runtimeResources();
   expect(calls[0][0]).toContain('/v1/workspaces/11111111-1111-4111-8111-111111111111/runtime-resources');
   expect(calls[0][1]).toBe('GET');
   expect(r.value.advice_is_operational_only).toBe(true);
   expect(r.value.establishes_epistemic_quality).toBe(false);
 });

});
