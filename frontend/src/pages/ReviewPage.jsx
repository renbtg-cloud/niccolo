import { FrozenContext } from './FrozenContext.jsx';
import { isCurrentSession, useSessionStore } from '../stores/session.js';
import { ClarificationCard } from './ClarificationCard.jsx';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { situationTextError, utf8ByteLength } from '../state/situationText.js';
import { Link } from 'react-router-dom';
import { useId, useRef, useState } from 'react';
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
import { handoffSummary, receiptSummary } from '../state/handoffPresentation.js';
import { reanalysisChanges } from '../state/reanalysisRevisionPresentation.js';
import { presentationZip } from '../state/presentationZip.js';

export function ReviewPage(){
 const selectedAnalysis=useUiStore(s=>s.analysis);
 return <ReviewContent key={selectedAnalysis||'unselected'} />;
}

function ReviewContent(){
 const pendingReanalysis=useRef(null);
 const pendingClarification=useRef(new Map());
 const pendingReceipt=useRef(null);
 const generation=useSessionStore(s=>s.generation);
 const client=useClient(); const analysis=useUiStore(s=>s.analysis); const activeOperation=useUiStore(s=>s.activeOperation); const setAnalysis=useUiStore(s=>s.setAnalysis); const setActiveOperation=useUiStore(s=>s.setActiveOperation); const dashboard=useDashboard(); const revision=useReanalysisRevision(analysis); const qc=useQueryClient(); const [reanalysisGoal,setReanalysisGoal]=useState('');
 const observed=useOperationObserver(activeOperation);
 const [questionPages,setQuestionPages]=useState([null]);
 const clarifications=useClarifications(questionPages.at(-1)); const reasoningInput=useReasoningInput(); const handoff=useReasoningHandoff(); const output=useAnalysisOutput(analysis, observed.operation?.target_analysis_id===analysis ? observed.operation : null); const receipts=useHandoffReceipts(); const [handoffNote,setHandoffNote]=useState('');
 const review=useQuery({queryKey:client&&analysis?qk.analysis(client.config,analysis,'review'):['review','none'],enabled:!!client&&!!analysis,queryFn:async ({ signal })=> (await client.analysisReview(analysis, signal)).value});
 const r=review.data; const d=dashboard.data; const handoffView=handoffSummary(handoff.data); const receiptView=receiptSummary(receipts.data);
 const reanalysisGoalErrorId=useId();
 const reanalysisGoalError=situationTextError(reanalysisGoal);
 const cleanHandoffNote=handoffNote.trim();
 const handoffNoteErrorId=useId();
 const handoffNoteError=cleanHandoffNote.includes('\0') ? 'The note cannot contain NUL.'
   : utf8ByteLength(cleanHandoffNote)>4096 ? 'The note must be at most 4096 UTF-8 bytes.' : null;
 const [clarificationDrafts,setClarificationDrafts]=useState({});
 // Keep every hook above conditional returns so connect/selection transitions never change hook order.
 const answerClarification=useMutation({mutationFn:async({questionId,text})=>{
   const signature=JSON.stringify({analysis,questionId,text});
   if(!pendingClarification.current.has(signature)){
     pendingClarification.current.set(signature,crypto.randomUUID());
   }
   const id=pendingClarification.current.get(signature);
   const result=await client.answerClarification(analysis,questionId,id,text);
   pendingClarification.current.delete(signature);
   return result.value;
 },onSuccess:async(_,variables)=>{
   if(!isCurrentSession(generation))return;
   setClarificationDrafts(current=>{
     if(current[variables.questionId]?.trim()!==variables.text) return current;
     const remaining={...current};
     delete remaining[variables.questionId];
     return remaining;
   });
   await Promise.all([qc.invalidateQueries({queryKey:['analysis']}),qc.invalidateQueries({queryKey:['dashboard']}),qc.invalidateQueries({queryKey:qk.situation(client.config),exact:true})]);
 }});
 const recordReceipt=useMutation({mutationFn:async(kind)=>{
   if(!handoffView?.sha256)throw new Error('No current handoff');
   const note=cleanHandoffNote||null;
   const signature=JSON.stringify({analysis,hash:handoffView.sha256,kind,note});
   if(pendingReceipt.current?.signature!==signature){
     pendingReceipt.current={signature,id:crypto.randomUUID()};
   }
   return (await client.recordHandoffReceipt(analysis,pendingReceipt.current.id,handoffView.sha256,kind,note)).value;
 },onSuccess:async()=>{
   if(!isCurrentSession(generation))return;
   pendingReceipt.current=null;
   setHandoffNote('');
   await qc.invalidateQueries({queryKey:qk.analysis(client.config,analysis,'handoff-receipts')});
 }});
 const exportPackage=useMutation({mutationFn:async()=>{
   const result=await client.presentationPackage(analysis);
   if (!isCurrentSession(generation)) return 0;
   const files=result.value?.files;
   const archive=presentationZip(result.value);
   const url=URL.createObjectURL(archive);
   const anchor=document.createElement('a');
   try {
     anchor.href=url;
     anchor.download=`niccolo-analysis-${analysis}.zip`;
     document.body.append(anchor);
     anchor.click();
   } finally {
     anchor.remove();
     window.setTimeout(()=>URL.revokeObjectURL(url),30000);
   }
   return Object.keys(files).length;
 }});
 const approveReanalysis=useMutation({mutationFn:async()=>{
   const goal=reanalysisGoal.trim()||r?.goal||'Revisit the prior analysis using the current Situation';
   const signature=JSON.stringify({analysis,goal});
   if(pendingReanalysis.current?.signature!==signature){
     pendingReanalysis.current={signature,ids:{snapshotOperation:crypto.randomUUID(),newAnalysis:crypto.randomUUID(),agenticOperation:crypto.randomUUID()}};
   }
   return launchApprovedReanalysis({client,priorAnalysis:analysis,goal,ids:pendingReanalysis.current.ids});
 },onSuccess:async(result)=>{if(!isCurrentSession(generation))return;pendingReanalysis.current=null;setAnalysis(result.newAnalysis);setActiveOperation(result.agenticOperation);await Promise.all([qc.invalidateQueries({queryKey:['analysis']}),qc.invalidateQueries({queryKey:['dashboard']}),qc.invalidateQueries({queryKey:['operation']}),qc.invalidateQueries({queryKey:qk.analysisHistory(client.config)})]);}});
 if(!client)return <p>Connect first.</p>;
 if(!analysis)return <section><h2>Human review</h2><p><Link to="/operations">Run an analysis</Link> or <Link to="/analysis">choose an earlier analysis</Link> to review.</p></section>;
 if(review.error && !r)return <section><h2>Human review</h2>
   <p className="error" role="alert">Review unavailable: {review.error.message}</p>
   <button type="button" disabled={review.isFetching} onClick={()=>void review.refetch()}>Retry review</button>
 </section>;
 const coverage=coverageSignals(r); const changes=recordedChanges(r); const leads=proactiveLeads(d); const reanalysis=reanalysisPrompt(d); const revisionChanges=reanalysisChanges(revision.data);

 return <section><h2>Human review</h2>
   {!r&&<p role="status">Loading review…</p>}
   {r&&review.isError&&<div>
     <p role="alert">Showing the last loaded review. Refresh failed.</p>
     <button type="button" disabled={review.isFetching} onClick={()=>void review.refetch()}>Retry review refresh</button>
   </div>}
  {r&&<>
     <div className="review-heading"><div><strong>{r.goal||'Analysis without an explicit goal'}</strong><div><small>Frozen at Situation revision {r.input_revision}; current revision {r.current_situation_revision}.</small></div></div><span className={r.input_is_current?'status-current':'status-historical'}>{r.input_is_current?'Current input':'Situation changed'}</span></div>
     {output.data&&<article className="review-panel review-answer"><h3>Niccolò's answer</h3><p className="answer-text">{output.data.answer_text}</p><p className="coverage-note">{output.data.coverage_note}</p>{!output.data.input_is_current&&<p className="historical-note">This answer belongs to historical input and is preserved rather than rewritten.</p>}</article>}
     {output.error && (output.data
       ? <p role="status">Showing the last loaded answer. Refresh failed.</p>
       : <div><p className="error" role="alert">Answer unavailable: {output.error.message}</p>
         <button type="button" disabled={output.isFetching} onClick={()=>void output.refetch()}>Retry answer</button></div>)}
     {output.isPending && <p role="status">Checking for a published answer…</p>}
     {!output.isPending && !output.error && !output.data && <p>No answer has been published for this analysis yet.</p>}
     <article className="review-panel"><h3>Worth looking at next</h3>{dashboard.error ? <div><p role="alert">Next steps unavailable: {dashboard.error.message}</p>
       <button type="button" disabled={dashboard.isFetching} onClick={()=>void dashboard.refetch()}>Retry next steps</button></div> : dashboard.isPending ? <p role="status">Checking next steps…</p> : leads.length? <div className="lead-list">{leads.map(lead=><div className="lead" key={lead.id}><strong>{lead.label}</strong><p>{lead.text}</p><small>{lead.whyNow}{lead.userMayIgnore?' · You may ignore this lead.':''}</small></div>)}</div> : <p>No investigation lead is available for this analysis yet.</p>}{d?.proactive?.truncated&&<p><small>Showing the highest-priority leads from {d.proactive.candidate_count} candidates.</small></p>}<Link to="/">Add records or human context to the Situation →</Link></article>
     <article className="review-panel"><h3>Download this analysis</h3>
       <p>Save a portable ZIP containing the recorded published analysis, CSV tables, a Python inspector and SHA-256 checksums. Original uploaded documents and complete model reasoning are not included. This does not upload anything to GitHub or another service.</p>
       <button disabled={!output.data||exportPackage.isPending} onClick={()=>exportPackage.mutate()}>{exportPackage.isPending?'Preparing ZIP…':'Download portable analysis ZIP'}</button>
       {exportPackage.error&&<p className="error" role="alert">Export unavailable: {exportPackage.error.message}</p>}
       {exportPackage.isSuccess&&exportPackage.data>0&&<p role="status">Archive prepared with {exportPackage.data} files. Keep it private unless you choose to share it.</p>}
     </article>
     {reasoningInput.isError&&<article className="review-panel">
       <h3>What reached the reasoning model</h3>
       <p role="alert">The recorded model input could not be loaded.</p>
       <button type="button" disabled={reasoningInput.isFetching} onClick={()=>void reasoningInput.refetch()}>Retry model input</button>
     </article>}
     {reasoningInput.data && !reasoningInput.isError&&<article className={`review-panel ${reasoningInput.data.source_packet_truncated?'input-warning':''}`}><h3>What reached the reasoning model</h3><p><strong>{reasoningInput.data.source_fragment_count_included}/{reasoningInput.data.source_fragment_count_total}</strong> frozen source fragment(s) included · {reasoningInput.data.source_bytes_included} source byte(s).</p>{reasoningInput.data.source_packet_truncated&&<p><strong>Input was truncated by the explicit reasoning budget.</strong> The model did not receive every frozen source fragment.</p>}<p><strong>{reasoningInput.data.identity_binding_count??0}</strong> human-confirmed identity binding(s) frozen at identity event #{reasoningInput.data.identity_event_no_max??0}.</p><p><small>Matching speaker labels do not imply the same person. Frozen revision {reasoningInput.data.input_revision} · packet SHA-256 <code>{reasoningInput.data.reasoning_packet_sha256}</code>. This is input observability, not a truth or completeness score.</small></p></article>}
     <div className="review-grid">
       <article><h3>What Niccolò actually used</h3>{coverage.map(x=><p key={x.label}><strong>{x.used}/{x.total}</strong> {x.label.toLowerCase()}</p>)}{r.sources?.opaque_pdfs > 0 && <p><strong>{r.sources.opaque_pdfs}</strong> attached PDF original(s). Selected passages and page descriptions are counted as human context. The reasoning model receives source fragments and human context, with PDF originals retained separately.</p>}<p><small>These are recorded coverage counts, not a completeness or truth score.</small></p></article>
       <article><h3>What changed afterward</h3>{changes.length? <ul>{changes.map(x=><li key={x}>{x}</li>)}</ul> : <p>No recorded Situation delta after the frozen input.</p>}</article>
     </div>
     {r.human_context?.present_in_input > 0 && <FrozenContext client={client} analysis={analysis}/>}
     {revision.isError&&<article className="review-panel">
       <h3>Recorded reanalysis changes</h3>
       <p role="alert">The comparison could not be loaded.</p>
       <button type="button" disabled={revision.isFetching} onClick={()=>void revision.refetch()}>Retry comparison</button>
     </article>}
     {revision.data && !revision.isError&&<article className="review-panel"><h3>Recorded reanalysis changes</h3><p>Compared with analysis <code>{revision.data.prior_analysis_id}</code>:</p>{revisionChanges.length?<ul>{revisionChanges.map(item=><li key={item.label}><strong>{item.count}</strong> {item.label.toLowerCase()}</li>)}</ul>:<p>No recorded input/claim-lineage change is present.</p>}{revision.data.unlinked_prior_claims?.length>0&&<p>{revision.data.unlinked_prior_claims.length} prior claim(s) have no recorded lineage link.</p>}{revision.data.unlinked_new_claims?.length>0&&<p>{revision.data.unlinked_new_claims.length} new claim(s) have no recorded lineage link.</p>}<small>Input changes do not prove why an output changed, and claims without lineage are not automatically classified as changes.</small></article>}
     <article className="review-panel"><h3>Reasoning handoff</h3>{handoff.isPending? <p>Loading deterministic handoff…</p> : handoff.error? <div><p className="error" role="alert">Handoff unavailable: {handoff.error.message}</p>
       <button type="button" disabled={handoff.isFetching} onClick={()=>void handoff.refetch()}>Retry handoff</button></div> : handoffView? <><p><strong>v{handoffView.version}</strong> · {handoffView.claimCount} claim(s) · {handoffView.candidateInterpretationCount} competing interpretation(s) · {handoffView.unansweredClarificationCount}/{handoffView.clarificationCount} clarification(s) still unanswered.</p>{handoffView.candidateInterpretations.length>0&&<div><h4>Competing interpretations considered</h4><ul>{handoffView.candidateInterpretations.map(item=><li key={item.id}><strong>{item.text}</strong><div><small>{item.rationale}</small></div></li>)}</ul><small>These remain candidates, not facts, and are not automatically ranked.</small></div>}<p><small>SHA-256 <code>{handoffView.sha256}</code></small></p><p><small>This handoff transfers inspectable reasoning state. It grants no authority, starts no analysis and executes no external action.</small></p><label>Optional receipt note<textarea aria-label="Handoff receipt note" value={handoffNote} disabled={recordReceipt.isPending} aria-invalid={Boolean(handoffNoteError)} aria-describedby={handoffNoteError?handoffNoteErrorId:undefined} onChange={e=>setHandoffNote(e.target.value)} placeholder="A receipt records that you received, reviewed or declined this exact handoff; it is not agreement."/></label>{handoffNoteError&&<p id={handoffNoteErrorId} role="alert">{handoffNoteError}</p>}<div className="row"><button disabled={recordReceipt.isPending||Boolean(handoffNoteError)} onClick={()=>recordReceipt.mutate('RECEIVED')}>Mark received</button><button disabled={recordReceipt.isPending||Boolean(handoffNoteError)} onClick={()=>recordReceipt.mutate('REVIEWED')}>Mark reviewed</button><button disabled={recordReceipt.isPending||Boolean(handoffNoteError)} onClick={()=>recordReceipt.mutate('DECLINED')}>Decline handoff</button></div>{recordReceipt.isSuccess&&<p role="status">Handoff receipt recorded.</p>}{recordReceipt.error&&<p className="error" role="alert">{recordReceipt.error.message}</p>}{receipts.isError && <div><p role="alert">Handoff receipts are unavailable.</p>
       <button type="button" disabled={receipts.isFetching} onClick={()=>void receipts.refetch()}>Retry receipts</button></div>}
       {receipts.isPending && <p role="status">Loading handoff receipts…</p>}
       {receipts.isSuccess && <p><small>Current handoff: {receiptView.currentTotal} receipt(s) — {receiptView.currentReceived} received · {receiptView.currentReviewed} reviewed · {receiptView.currentDeclined} declined. {receiptView.historicalTotal} historical receipt(s) refer to earlier handoff hashes. Receipt never means agreement and never grants authority.</small></p>}{receiptView.events.length>0&&<ul>{receiptView.events.map(event=><li key={event.receiptId}><strong>{event.kind.toLowerCase()}</strong> · {event.current?'current handoff':'prior handoff'} · {event.createdAt}<div><small>Recipient <code>{event.recipient}</code>{event.note?` · ${event.note}`:''}</small></div></li>)}</ul>}</> : <p>No handoff is available for this analysis.</p>}</article>
     <article className="review-panel"><h3>Clarifications</h3>{clarifications.isPending? <p>Loading clarification questions…</p> : clarifications.error? <div>
       <p className="error" role="alert">Clarifications unavailable: {clarifications.error.message}</p>
       <button type="button" disabled={clarifications.isFetching} onClick={()=>void clarifications.refetch()}>Retry loading questions</button>
     </div> : clarifications.data?.clarifications?.length? <div className="clarification-list">{clarifications.data.clarifications.map(question=><ClarificationCard
         key={question.question_id} question={question} draft={clarificationDrafts[question.question_id]??''}
         onChange={text=>setClarificationDrafts(current=>({...current,[question.question_id]:text}))}
         onSubmit={text=>answerClarification.mutate({questionId:question.question_id,text})}
         saving={answerClarification.isPending}
       />)}</div> : <p>No clarification question is currently recorded.</p>}
       {questionPages.length>1 && <button type="button" disabled={clarifications.isFetching} onClick={()=>setQuestionPages(pages=>pages.slice(0,-1))}>Previous questions</button>}
       {clarifications.data?.next_after && <button type="button" disabled={clarifications.isFetching} onClick={()=>setQuestionPages(pages=>[...pages,clarifications.data.next_after])}>More questions</button>}<p><small>Clarification questions are not evidence. Answers are new attributed human context and never rewrite the analysis input that motivated the question.</small></p>{answerClarification.isSuccess&&<p role="status">Answer saved as human context.</p>}{answerClarification.error&&<p className="error" role="alert">{answerClarification.error.message}</p>}</article>
     <article className="review-panel"><h3>Reanalysis</h3>{dashboard.error ? <p>Reanalysis recommendations are unavailable.</p> : dashboard.isPending ? <p>Checking for changes…</p> : reanalysis? <><p>The backend reports material post-analysis change:</p><ul>{reanalysis.reasons.map(x=><li key={x}>{x}</li>)}</ul><p><strong>Your approval is required.</strong> This recommendation does not start a new analysis automatically, and the old output remains preserved.</p><label>Goal for the new analysis<textarea aria-label="Reanalysis goal" value={reanalysisGoal} disabled={approveReanalysis.isPending} aria-invalid={Boolean(reanalysisGoalError)} aria-describedby={reanalysisGoalError?reanalysisGoalErrorId:undefined} onChange={e=>setReanalysisGoal(e.target.value)} placeholder={r.goal||'What should the new analysis revisit?'}/></label>{reanalysisGoalError&&<p id={reanalysisGoalErrorId} role="alert">{reanalysisGoalError}</p>}<button disabled={approveReanalysis.isPending||Boolean(reanalysisGoalError)} onClick={()=>approveReanalysis.mutate()}>{approveReanalysis.isPending?'Starting reanalysis…':'Approve and start reanalysis'}</button>{approveReanalysis.error&&<p className="error" role="alert">{approveReanalysis.error.message}</p>}</> : <p>No reanalysis recommendation is currently recorded.</p>}</article>
     <details className="desktop-only"><summary>Raw review data</summary><div className="inspector-grid"><pre>{JSON.stringify(r,null,2)}</pre><pre>{JSON.stringify(d,null,2)}</pre></div></details>
   </>}
 </section>;
}
