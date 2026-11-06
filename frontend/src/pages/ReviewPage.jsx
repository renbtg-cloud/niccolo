import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { qk } from '../api/keys.js';
import { useClient } from '../hooks/useClient.js';
import { useDashboard } from '../hooks/useDashboard.js';
import { useUiStore } from '../stores/ui.js';
import { coverageSignals, recordedChanges, proactiveLeads, reanalysisPrompt } from '../state/reviewPresentation.js';
import { launchApprovedReanalysis } from '../state/reanalysisLaunch.js';

export function ReviewPage(){
 const client=useClient(); const analysis=useUiStore(s=>s.analysis); const setAnalysis=useUiStore(s=>s.setAnalysis); const setActiveOperation=useUiStore(s=>s.setActiveOperation); const dashboard=useDashboard(); const qc=useQueryClient(); const [reanalysisGoal,setReanalysisGoal]=useState('');
 const review=useQuery({queryKey:client&&analysis?qk.analysis(client.config,analysis,'review'):['review','none'],enabled:!!client&&!!analysis,queryFn:async()=> (await client.analysisReview(analysis)).value});
 if(!client)return <p>Connect first.</p>;
 if(!analysis)return <section><h2>Human review</h2><p>Select or run an analysis first.</p></section>;
 if(review.error||dashboard.error)return <section><h2>Human review</h2><p className="error">Review unavailable: {(review.error??dashboard.error).message}</p></section>;
 const r=review.data; const d=dashboard.data;
 const coverage=coverageSignals(r); const changes=recordedChanges(r); const leads=proactiveLeads(d); const reanalysis=reanalysisPrompt(d);
 const approveReanalysis=useMutation({mutationFn:async()=>launchApprovedReanalysis({client,priorAnalysis:analysis,goal:reanalysisGoal.trim()||r?.goal||'Revisit the prior analysis using the current Situation'}),onSuccess:async(result)=>{setAnalysis(result.newAnalysis);setActiveOperation(result.agenticOperation);await Promise.all([qc.invalidateQueries({queryKey:['analysis']}),qc.invalidateQueries({queryKey:['dashboard']}),qc.invalidateQueries({queryKey:['operation']})]);}});

 return <section><h2>Human review</h2>
   {!r&&<p>Loading review…</p>}
  {r&&<>
     <div className="review-heading"><div><strong>{r.goal||'Analysis without an explicit goal'}</strong><div><small>Frozen at Situation revision {r.input_revision}; current revision {r.current_revision}.</small></div></div><span className={r.input_is_current?'status-current':'status-historical'}>{r.input_is_current?'Current input':'Situation changed'}</span></div>
     <div className="review-grid">
       <article><h3>What Niccolò actually used</h3>{coverage.map(x=><p key={x.label}><strong>{x.used}/{x.total}</strong> {x.label.toLowerCase()}</p>)}<p><small>These are recorded coverage counts, not a completeness or truth score.</small></p></article>
       <article><h3>What changed afterward</h3>{changes.length? <ul>{changes.map(x=><li key={x}>{x}</li>)}</ul> : <p>No recorded Situation delta after the frozen input.</p>}</article>
     </div>
     <article className="review-panel"><h3>Worth looking at next</h3>{leads.length? <div className="lead-list">{leads.map(lead=><div className="lead" key={lead.id}><strong>{lead.kind.replaceAll('_',' ')}</strong><p>{lead.text}</p><small>{lead.whyNow}{lead.userMayIgnore?' · You may ignore this lead.':''}</small></div>)}</div> : <p>No proactive lead is currently recorded.</p>}<p><small>These are prioritization prompts from the backend; they are not evidence and do not establish truth.</small></p></article>
     <article className="review-panel"><h3>Reanalysis</h3>{reanalysis? <><p>The backend reports material post-analysis change:</p><ul>{reanalysis.reasons.map(x=><li key={x}>{x}</li>)}</ul><p><strong>Your approval is required.</strong> This recommendation does not start a new analysis automatically, and the old output remains preserved.</p><label>Goal for the new analysis<textarea aria-label="Reanalysis goal" value={reanalysisGoal} onChange={e=>setReanalysisGoal(e.target.value)} placeholder={r.goal||'What should the new analysis revisit?'}/></label><button disabled={approveReanalysis.isPending} onClick={()=>approveReanalysis.mutate()}>{approveReanalysis.isPending?'Starting reanalysis…':'Approve and start reanalysis'}</button>{approveReanalysis.error&&<p className="error">{approveReanalysis.error.message}</p>}</> : <p>No reanalysis recommendation is currently recorded.</p>}</article>
     <details className="desktop-only"><summary>Raw review data</summary><div className="inspector-grid"><pre>{JSON.stringify(r,null,2)}</pre><pre>{JSON.stringify(d,null,2)}</pre></div></details>
   </>}
 </section>;
}
