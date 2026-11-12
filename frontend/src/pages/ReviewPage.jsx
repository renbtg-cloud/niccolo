import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useRef, useState } from 'react';
import { qk } from '../api/keys.js';
import { useClient } from '../hooks/useClient.js';
import { useDashboard } from '../hooks/useDashboard.js';
import { useUiStore } from '../stores/ui.js';
import { coverageSignals, recordedChanges, proactiveLeads, reanalysisPrompt } from '../state/reviewPresentation.js';
import { launchApprovedReanalysis } from '../state/reanalysisLaunch.js';
import { useReanalysisRevision } from '../hooks/useReanalysisRevision.js';
import { useClarifications } from '../hooks/useClarifications.js';
import { useReasoningInput } from '../hooks/useReasoningInput.js';
import { useReasoningHandoff } from '../hooks/useReasoningHandoff.js';
import { useAnalysisOutput } from '../hooks/useAnalysisOutput.js';
import { useOperationObserver } from '../hooks/useOperationObserver.js';
import { useHandoffReceipts } from '../hooks/useHandoffReceipts.js';
import { useForensicContext } from '../hooks/useForensicContext.js';
import { handoffSummary, receiptSummary } from '../state/handoffPresentation.js';
import { reanalysisChanges } from '../state/reanalysisRevisionPresentation.js';
import { presentationZip } from '../state/presentationZip.js';

function downloadPresentationPackage(pkg, filename) {
 const files=pkg?.files;
 const archive=presentationZip(pkg);
 const url=URL.createObjectURL(archive);
 try {
   const anchor=document.createElement('a');
   anchor.href=url;
   anchor.download=filename;
   document.body.append(anchor);
   anchor.click();
   anchor.remove();
 } finally {
   window.setTimeout(()=>URL.revokeObjectURL(url),30000);
 }
 return Object.keys(files??{}).length;
}

export function ReviewPage(){
 const selectedAnalysis=useUiStore(s=>s.analysis);
 return <ReviewContent key={selectedAnalysis||'unselected'} />;
}

function ReviewContent(){
 const pendingReanalysis=useRef(null);
 const pendingClarification=useRef(null);
 const pendingReceipt=useRef(null);
 const client=useClient(); const analysis=useUiStore(s=>s.analysis); const activeOperation=useUiStore(s=>s.activeOperation); const setAnalysis=useUiStore(s=>s.setAnalysis); const setActiveOperation=useUiStore(s=>s.setActiveOperation); const dashboard=useDashboard(); const revision=useReanalysisRevision(analysis); const qc=useQueryClient(); const [reanalysisGoal,setReanalysisGoal]=useState('');
 const observed=useOperationObserver(activeOperation);
 const clarifications=useClarifications(); const reasoningInput=useReasoningInput(); const forensicContext=useForensicContext(); const handoff=useReasoningHandoff(); const output=useAnalysisOutput(analysis, observed.operation?.target_analysis_id===analysis ? observed.operation : null); const receipts=useHandoffReceipts(); const [handoffNote,setHandoffNote]=useState('');
 const review=useQuery({queryKey:client&&analysis?qk.analysis(client.config,analysis,'review'):['review','none'],enabled:!!client&&!!analysis,queryFn:async()=> (await client.analysisReview(analysis)).value});
 const r=review.data; const d=dashboard.data; const handoffView=handoffSummary(handoff.data); const receiptView=receiptSummary(receipts.data);
 const [clarificationDrafts,setClarificationDrafts]=useState({});
 // Keep every hook above conditional returns so connect/selection transitions never change hook order.
 const answerClarification=useMutation({mutationFn:async({questionId,text})=>{
   const signature=JSON.stringify({analysis,questionId,text});
   if(pendingClarification.current?.signature!==signature){
     pendingClarification.current={signature,id:crypto.randomUUID()};
   }
   return (await client.answerClarification(analysis,questionId,pendingClarification.current.id,text)).value;
 },onSuccess:async(_,variables)=>{
   pendingClarification.current=null;
   setClarificationDrafts(current=>({...current,[variables.questionId]:''}));
   await Promise.all([qc.invalidateQueries({queryKey:['analysis']}),qc.invalidateQueries({queryKey:['dashboard']})]);
 }});
 const recordReceipt=useMutation({mutationFn:async(kind)=>{
   if(!handoffView?.sha256)throw new Error('No current handoff');
   const note=handoffNote.trim()||null;
   const signature=JSON.stringify({analysis,hash:handoffView.sha256,kind,note});
   if(pendingReceipt.current?.signature!==signature){
     pendingReceipt.current={signature,id:crypto.randomUUID()};
   }
   return (await client.recordHandoffReceipt(analysis,pendingReceipt.current.id,handoffView.sha256,kind,note)).value;
 },onSuccess:async()=>{
   pendingReceipt.current=null;
   setHandoffNote('');
   await qc.invalidateQueries({queryKey:qk.analysis(client.config,analysis,'handoff-receipts')});
 }});
 const exportPackage=useMutation({mutationFn:async()=>{
   const result=await client.presentationPackage(analysis);
   return downloadPresentationPackage(result.value,`niccolo-analysis-${analysis}.zip`);
 }});
 const exportPackageWithSources=useMutation({mutationFn:async()=>{
   const result=await client.presentationPackageWithExaminedSources(analysis);
   return downloadPresentationPackage(result.value,`niccolo-analysis-${analysis}-with-examined-sources.zip`);
 }});
 const approveReanalysis=useMutation({mutationFn:async()=>{
   const goal=reanalysisGoal.trim()||r?.goal||'Revisit the prior analysis using the current Situation';
   const signature=JSON.stringify({analysis,goal});
   if(pendingReanalysis.current?.signature!==signature){
     pendingReanalysis.current={signature,ids:{snapshotOperation:crypto.randomUUID(),newAnalysis:crypto.randomUUID(),agenticOperation:crypto.randomUUID()}};
   }
   return launchApprovedReanalysis({client,priorAnalysis:analysis,goal,ids:pendingReanalysis.current.ids});
 },onSuccess:async(result)=>{pendingReanalysis.current=null;setAnalysis(result.newAnalysis);setActiveOperation(result.agenticOperation);await Promise.all([qc.invalidateQueries({queryKey:['analysis']}),qc.invalidateQueries({queryKey:['dashboard']}),qc.invalidateQueries({queryKey:['operation']})]);}});
 if(!client)return <p>Connect first.</p>;
 if(!analysis)return <section><h2>Human review</h2><p>Select or run an analysis first.</p></section>;
 if(review.error||dashboard.error)return <section><h2>Human review</h2><p className="error">Review unavailable: {(review.error??dashboard.error).message}</p></section>;
 const coverage=coverageSignals(r); const changes=recordedChanges(r); const leads=proactiveLeads(d); const reanalysis=reanalysisPrompt(d); const revisionChanges=reanalysisChanges(revision.data);

 return <section><h2>Human review</h2>
   {!r&&<p>Loading review…</p>}
  {r&&<>
     <div className="review-heading"><div><strong>{r.goal||'Analysis without an explicit goal'}</strong><div><small>Frozen at Situation revision {r.input_revision}; current revision {r.current_revision}.</small></div></div><span className={r.input_is_current?'status-current':'status-historical'}>{r.input_is_current?'Current input':'Situation changed'}</span></div>
     {output.data&&<article className="review-panel review-answer"><h3>Niccolò's answer</h3><p className="answer-text">{output.data.answer_text}</p><p className="coverage-note">{output.data.coverage_note}</p>{!output.data.input_is_current&&<p className="historical-note">This answer belongs to historical input and is preserved rather than rewritten.</p>}</article>}
     {output.error&&<p className="error">Answer unavailable: {output.error.message}</p>}
     <article className="review-panel"><h3>Download this analysis</h3>
       <p>Save a portable ZIP containing the recorded published analysis, CSV tables, a Python inspector and SHA-256 checksums. The standard package contains source IDs but not source text, original uploaded documents or complete model reasoning. This does not upload anything to GitHub or another service.</p>
       <button disabled={!output.data||exportPackage.isPending||exportPackageWithSources.isPending} onClick={()=>exportPackage.mutate()}>{exportPackage.isPending?'Preparing ZIP…':'Download portable analysis ZIP'}</button>
       {exportPackage.error&&<p className="error" role="alert">Export unavailable: {exportPackage.error.message}</p>}
       {exportPackage.isSuccess&&<p role="status">Archive prepared with {exportPackage.data} files. Keep it private unless you choose to share it.</p>}
       <details className="sensitive-export"><summary>Include examined source text</summary>
         <p><strong>Sensitive export.</strong> This explicitly includes only source fragments recorded as examined by this analysis. It does not include original uploaded files, does not prove those excerpts are true, and may expose private communications. Inspect the ZIP before sharing it.</p>
         <button disabled={!output.data||exportPackage.isPending||exportPackageWithSources.isPending} onClick={()=>exportPackageWithSources.mutate()}>{exportPackageWithSources.isPending?'Preparing sensitive ZIP…':'Download ZIP with examined source excerpts'}</button>
         {exportPackageWithSources.error&&<p className="error" role="alert">Sensitive export unavailable: {exportPackageWithSources.error.message}</p>}
         {exportPackageWithSources.isSuccess&&<p role="status">Sensitive archive prepared with {exportPackageWithSources.data} files. No remote publication occurred.</p>}
       </details>
     </article>
     {reasoningInput.data&&<article className={`review-panel ${reasoningInput.data.source_packet_truncated?'input-warning':''}`}><h3>What reached the reasoning model</h3><p><strong>{reasoningInput.data.source_fragment_count_included}/{reasoningInput.data.source_fragment_count_total}</strong> frozen source fragment(s) included · {reasoningInput.data.source_bytes_included} source byte(s).</p>{reasoningInput.data.source_packet_truncated&&<p><strong>Input was truncated by the explicit reasoning budget.</strong> The model did not receive every frozen source fragment.</p>}<p><strong>{reasoningInput.data.identity_binding_count??0}</strong> human-confirmed identity binding(s) frozen at identity event #{reasoningInput.data.identity_event_no_max??0}.</p><p><small>Matching speaker labels do not imply the same person. Frozen revision {reasoningInput.data.input_revision} · packet SHA-256 <code>{reasoningInput.data.reasoning_packet_sha256}</code>. This is input observability, not a truth or completeness score.</small></p></article>}
     {forensicContext.error&&<p className="error">Imported historical context unavailable: {forensicContext.error.message}</p>}
     {forensicContext.data?.items?.length>0&&<article className="review-panel"><h3>Imported historical context</h3><p><strong>{forensicContext.data.items.length}</strong> quarantined item(s) were explicitly selected by a human for this analysis.</p><ul>{forensicContext.data.items.slice(0,8).map(item=><li key={item.binding_id}><strong>{item.source_path}</strong>{item.note&&<div>{item.note}</div>}<small>{item.origin_class.replaceAll('_',' ').toLowerCase()} · quarantine provenance: yes · documentary evidence: no · establishes truth: no</small></li>)}</ul>{forensicContext.data.items.length>8&&<p><small>{forensicContext.data.items.length-8} additional imported context item(s) are hidden from this compact review.</small></p>}<p><small>These items are current non-evidence context. They are not part of the frozen analysis input, do not upgrade historical assertions to facts, and never rewrite a published answer.</small></p><a href="/forensic">Inspect or add prior-export context</a></article>}
     <div className="review-grid">
       <article><h3>What Niccolò actually used</h3>{coverage.map(x=><p key={x.label}><strong>{x.used}/{x.total}</strong> {x.label.toLowerCase()}</p>)}<p><small>These are recorded coverage counts, not a completeness or truth score.</small></p></article>
       <article><h3>What changed afterward</h3>{changes.length? <ul>{changes.map(x=><li key={x}>{x}</li>)}</ul> : <p>No recorded Situation delta after the frozen input.</p>}</article>
     </div>
     <article className="review-panel"><h3>Worth looking at next</h3>{leads.length? <div className="lead-list">{leads.map(lead=><div className="lead" key={lead.id}><strong>{lead.kind.replaceAll('_',' ')}</strong><p>{lead.text}</p><small>{lead.whyNow}{lead.userMayIgnore?' · You may ignore this lead.':''}</small></div>)}</div> : <p>No proactive lead is currently recorded.</p>}<p><small>These are prioritization prompts from the backend; they are not evidence and do not establish truth.</small></p></article>
     {revision.data&&<article className="review-panel"><h3>Recorded reanalysis changes</h3><p>Compared with analysis <code>{revision.data.prior_analysis_id}</code>:</p>{revisionChanges.length?<ul>{revisionChanges.map(item=><li key={item.label}><strong>{item.count}</strong> {item.label.toLowerCase()}</li>)}</ul>:<p>No recorded input/claim-lineage change is present.</p>}{revision.data.unlinked_prior_claims?.length>0&&<p>{revision.data.unlinked_prior_claims.length} prior claim(s) have no recorded lineage link.</p>}{revision.data.unlinked_new_claims?.length>0&&<p>{revision.data.unlinked_new_claims.length} new claim(s) have no recorded lineage link.</p>}<small>Input changes do not prove why an output changed, and claims without lineage are not automatically classified as changes.</small></article>}
     <article className="review-panel"><h3>Reasoning handoff</h3>{handoff.isPending? <p>Loading deterministic handoff…</p> : handoff.error? <p className="error">Handoff unavailable: {handoff.error.message}</p> : handoffView? <><p><strong>v{handoffView.version}</strong> · {handoffView.claimCount} claim(s) · {handoffView.candidateInterpretationCount} competing interpretation(s) · {handoffView.unansweredClarificationCount}/{handoffView.clarificationCount} clarification(s) still unanswered.</p>{handoffView.candidateInterpretations.length>0&&<div><h4>Competing interpretations considered</h4><ul>{handoffView.candidateInterpretations.map(item=><li key={item.id}><strong>{item.text}</strong><div><small>{item.rationale}</small></div></li>)}</ul><small>These remain candidates, not facts, and are not automatically ranked.</small></div>}<p><small>SHA-256 <code>{handoffView.sha256}</code></small></p><p><small>This handoff transfers inspectable reasoning state. It grants no authority, starts no analysis and executes no external action.</small></p><label>Optional receipt note<textarea aria-label="Handoff receipt note" value={handoffNote} onChange={e=>setHandoffNote(e.target.value)} placeholder="A receipt records that you received, reviewed or declined this exact handoff; it is not agreement."/></label><div className="row"><button disabled={recordReceipt.isPending} onClick={()=>recordReceipt.mutate('RECEIVED')}>Mark received</button><button disabled={recordReceipt.isPending} onClick={()=>recordReceipt.mutate('REVIEWED')}>Mark reviewed</button><button disabled={recordReceipt.isPending} onClick={()=>recordReceipt.mutate('DECLINED')}>Decline handoff</button></div>{recordReceipt.error&&<p className="error">{recordReceipt.error.message}</p>}<p><small>Current handoff: {receiptView.currentTotal} receipt(s) — {receiptView.currentReceived} received · {receiptView.currentReviewed} reviewed · {receiptView.currentDeclined} declined. {receiptView.historicalTotal} historical receipt(s) refer to earlier handoff hashes. Receipt never means agreement and never grants authority.</small></p>{receiptView.events.length>0&&<ul>{receiptView.events.map(event=><li key={event.receiptId}><strong>{event.kind.toLowerCase()}</strong> · {event.current?'current handoff':'prior handoff'} · {event.createdAt}<div><small>Recipient <code>{event.recipient}</code>{event.note?` · ${event.note}`:''}</small></div></li>)}</ul>}</> : <p>No handoff is available for this analysis.</p>}</article>
     <article className="review-panel"><h3>Clarifications</h3>{clarifications.isPending? <p>Loading clarification questions…</p> : clarifications.error? <p className="error">Clarifications unavailable: {clarifications.error.message}</p> : clarifications.data?.clarifications?.length? <div className="clarification-list">{clarifications.data.clarifications.map(question=><div className="clarification" key={question.question_id}><strong>{question.text}</strong><p><small>Why asked: {question.reason} · based on frozen revision {question.input_revision}{question.input_is_current?'':' · Situation has changed since then'}</small></p>{question.answers?.length>0&&<ul>{question.answers.map(answer=><li key={answer.answer_id}>{answer.text}<div><small>Recorded later as {answer.kind}; not evidence and not part of the original frozen input.</small></div></li>)}</ul>}<label>Your answer<textarea aria-label={`Clarification answer ${question.question_id}`} value={clarificationDrafts[question.question_id]??''} onChange={e=>setClarificationDrafts(current=>({...current,[question.question_id]:e.target.value}))} placeholder="Answer in your own words; this becomes attributed context."/></label><button disabled={answerClarification.isPending||!(clarificationDrafts[question.question_id]??'').trim()} onClick={()=>answerClarification.mutate({questionId:question.question_id,text:(clarificationDrafts[question.question_id]??'').trim()})}>Add answer as context</button></div>)}</div> : <p>No clarification question is currently recorded.</p>}<p><small>Clarification questions are not evidence. Answers are new attributed human context and never rewrite the analysis input that motivated the question.</small></p>{answerClarification.error&&<p className="error">{answerClarification.error.message}</p>}</article>
     <article className="review-panel"><h3>Reanalysis</h3>{reanalysis? <><p>The backend reports material post-analysis change:</p><ul>{reanalysis.reasons.map(x=><li key={x}>{x}</li>)}</ul><p><strong>Your approval is required.</strong> This recommendation does not start a new analysis automatically, and the old output remains preserved.</p><label>Goal for the new analysis<textarea aria-label="Reanalysis goal" value={reanalysisGoal} onChange={e=>setReanalysisGoal(e.target.value)} placeholder={r.goal||'What should the new analysis revisit?'}/></label><button disabled={approveReanalysis.isPending} onClick={()=>approveReanalysis.mutate()}>{approveReanalysis.isPending?'Starting reanalysis…':'Approve and start reanalysis'}</button>{approveReanalysis.error&&<p className="error">{approveReanalysis.error.message}</p>}</> : <p>No reanalysis recommendation is currently recorded.</p>}</article>
     <details className="desktop-only"><summary>Raw review data</summary><div className="inspector-grid"><pre>{JSON.stringify(r,null,2)}</pre><pre>{JSON.stringify(d,null,2)}</pre></div></details>
   </>}
 </section>;
}
