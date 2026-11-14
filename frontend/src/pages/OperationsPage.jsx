import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useRef, useState } from 'react';
import { Link } from 'react-router-dom';
import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { z } from 'zod';
import { useClient } from '../hooks/useClient.js';
import { qk } from '../api/keys.js';
import { useUiStore } from '../stores/ui.js';
import { useSessionStore } from '../stores/session.js';
import { situationGuide } from '../state/situationGuide.js';
import { useOperationObserver } from '../hooks/useOperationObserver.js';
import { useAgenticPlan } from '../hooks/useAgenticPlan.js';
import { useAnalysisOutput } from '../hooks/useAnalysisOutput.js';
import { useOperationCheckpoints } from '../hooks/useOperationCheckpoints.js';
import { checkpointZip } from '../state/presentationZip.js';
import { CheckpointMilestonePublicationPanel } from '../components/CheckpointMilestonePublicationPanel.jsx';
import { RuntimeCapacityPanel } from '../components/RuntimeCapacityPanel.jsx';
import { useRuntimeResources } from '../hooks/useRuntimeResources.js';

const intentSchema=z.object({
  mode:z.enum(['sync','async']),
  intent:z.enum(['ABOUT_TO_ACT_OR_SAY','UNDERSTAND','FIGURE_OUT_WHAT_MATTERS']),
  goal:z.string().max(4096).optional(),
  stepBudget:z.coerce.number().int().min(1).max(10000),
  maxInvocations:z.coerce.number().int().min(1).max(1024),
  maxTotalTokens:z.coerce.number().int().min(1).max(10000000),
  finalizationReserveMs:z.coerce.number().int().min(0).max(300000),
});
const humanSchema=z.object({decision:z.enum(['CONTINUE','STOP','REPLAN']),responseText:z.string().trim().max(4096)}).refine(v=>v.decision==='STOP'||v.responseText.length>0,{path:['responseText']});

function downloadCheckpointPackage(pkg, filename) {
 const archive=checkpointZip(pkg);
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
 return Object.keys(pkg?.files??{}).length;
}

function AnswerPanel({ output, operation, error }) {
 if(error)return <p className="error">Answer unavailable: {error.message}</p>;
 if(output)return <article className="analysis-answer">
   <div className="answer-heading"><h3>Niccolò's answer</h3><span className={output.input_is_current?'status-current':'status-historical'}>{output.input_is_current?'Current input':'Historical input'}</span></div>
   <div className="answer-text">{output.answer_text}</div>
   <h4>Coverage</h4><div className="coverage-note">{output.coverage_note}</div>
   {!output.input_is_current&&<p className="historical-note">The Situation changed after this analysis input was frozen. The original answer is preserved.</p>}
   <p><Link to="/review">Review reasoning, clarifications and next steps →</Link></p>
 </article>;
 if(!operation)return <p>Start an analysis to get a provisional answer.</p>;
 if(operation.state==='WAITING_FOR_HUMAN')return <p>Niccolò needs your decision before it can continue the current reasoning plan.</p>;
 if(operation.state==='FAILED')return <p className="error" role="alert">Analysis failed{operation.error_code ? ` (${operation.error_code})` : ''}. No answer was published. You can start another analysis.</p>;
 if(operation.state==='CANCELLED')return <p role="status">This analysis was cancelled. No answer was published. You can start another analysis.</p>;
 if(operation.result?.completion_status==='PARTIAL_BUDGET_BOUNDARY')return <article className="analysis-answer provisional-answer" role="status">
   <div className="answer-heading"><h3>Useful work so far</h3><span className="status-historical">Provisional</span></div>
   <p>Niccolò stopped before another model call because the configured reasoning budget reached <strong>{operation.result.stop_reason?.replaceAll('_',' ').toLowerCase()}</strong>.</p>
   <p>The work already completed was preserved. This partial result does <strong>not</strong> claim completeness or factual truth, and it does not authorize any external action.</p>
   {operation.result.next_unexecuted_step_kind&&<p><strong>Next unexecuted reasoning step:</strong> {operation.result.next_unexecuted_step_kind.replaceAll('_',' ').toLowerCase()}</p>}
   <p><Link to="/review">Review the preserved reasoning state and next steps →</Link></p>
 </article>;
 if(operation.terminal)return <p role="status">This run finished without a published synthesis. You can start another analysis.</p>;
 return <p>Niccolò is working from the frozen analysis input. The answer will appear here when synthesis is published.</p>;
}

export function OperationsPage(){
 const client=useClient(); const qc=useQueryClient();
 const sessionGeneration=useSessionStore(s=>s.generation);
 const situation=useQuery({
   queryKey:client?qk.situation(client.config):['situation','none'],
   enabled:!!client,
   queryFn:async()=> (await client.situation()).value,
   retry:false,
 });
 const readiness=useQuery({
   queryKey:qk.readiness(sessionGeneration),
   enabled:!!client,
   queryFn:async()=> (await client.readiness()).value,
   staleTime:60000,
   retry:false,
 });
 const modelUnavailable=readiness.data?.model_configured===false;
 const situationMissing=situation.error?.status===404;
 const situationUnavailable=!!situation.error&&!situationMissing;
 const guide=situation.data?situationGuide(situation.data):null;
 const pendingLaunch=useRef(null);
 const pendingHumanResponse=useRef(null);
 const pendingMilestone=useRef(null);
 const active=useUiStore(s=>s.activeOperation); const analysis=useUiStore(s=>s.analysis); const setActive=useUiStore(s=>s.setActiveOperation); const setAnalysis=useUiStore(s=>s.setAnalysis);
 const observed=useOperationObserver(active); const plan=useAgenticPlan(active,observed.operation?.state); const checkpoints=useOperationCheckpoints(active); const output=useAnalysisOutput(analysis,observed.operation); const resources=useRuntimeResources();
 const budget=useQuery({queryKey:client&&active?['operation-budget',client.config.workspace,client.config.situation,active]:['operation-budget','none'],enabled:!!client&&!!active,queryFn:async()=> (await client.operationBudget(active)).value,refetchInterval:observed.operation?.terminal?false:2000});
 const latestCheckpoint=checkpoints.data?.checkpoints?.at(-1)??null;
 const [milestoneKind,setMilestoneKind]=useState('FIRST_PASS');
 const [milestoneReason,setMilestoneReason]=useState('');
 const latestMilestone=useQuery({
   queryKey:client&&active&&latestCheckpoint?['operation-checkpoint-milestone',client.config.workspace,client.config.situation,active,latestCheckpoint.checkpoint_id]:['operation-checkpoint-milestone','none'],
   enabled:!!client&&!!active&&!!latestCheckpoint,
   queryFn:async()=>{try{return (await client.operationCheckpointMilestone(active,latestCheckpoint.checkpoint_id)).value;}catch(error){if(error?.status===404)return null;throw error;}},
   retry:false,
 });
 const declareMilestone=useMutation({mutationFn:async()=>{
   if(!active||!latestCheckpoint)throw new Error('No checkpoint available');
   const reason=milestoneReason.trim();
   if(!reason)throw new Error('Explain why this checkpoint is a meaningful milestone.');
   const signature=JSON.stringify({active,checkpoint:latestCheckpoint.checkpoint_id,milestoneKind,reason});
   if(pendingMilestone.current?.signature!==signature){
     pendingMilestone.current={signature,milestoneId:crypto.randomUUID()};
   }
   return (await client.declareOperationCheckpointMilestone(active,latestCheckpoint.checkpoint_id,pendingMilestone.current.milestoneId,milestoneKind,reason)).value;
 },onSuccess:async()=>{pendingMilestone.current=null;setMilestoneReason('');await qc.invalidateQueries({queryKey:['operation-checkpoint-milestone']});}});
 const exportCheckpoint=useMutation({mutationFn:async({checkpoint,milestone=null})=>{if(!active)throw new Error('No active operation');const result=await client.operationCheckpointPackage(active,checkpoint.checkpoint_id,milestone?.milestone_id??null);const suffix=milestone?'-milestone-'+milestone.milestone_id:'';return downloadCheckpointPackage(result.value,'niccolo-operation-'+active+'-checkpoint-'+checkpoint.checkpoint_seq+suffix+'.zip');}});
 const {register,handleSubmit,formState:{errors}}=useForm({resolver:zodResolver(intentSchema),defaultValues:{mode:'sync',intent:'UNDERSTAND',goal:'',stepBudget:32,maxInvocations:32,maxTotalTokens:200000,finalizationReserveMs:5000}});
 const humanForm=useForm({resolver:zodResolver(humanSchema),defaultValues:{decision:'CONTINUE',responseText:''}});
 const run=useMutation({mutationFn:async(v)=>{
   const signature=JSON.stringify({mode:v.mode,intent:v.intent,goal:v.goal?.trim()||null,stepBudget:v.stepBudget,maxInvocations:v.maxInvocations,maxTotalTokens:v.maxTotalTokens,finalizationReserveMs:v.finalizationReserveMs});
   if(pendingLaunch.current?.signature!==signature){
     pendingLaunch.current={signature,operationId:crypto.randomUUID(),analysisId:crypto.randomUUID()};
   }
   const {operationId,analysisId}=pendingLaunch.current;
   const task={analysis_id:analysisId,intent:v.intent,goal:v.goal?.trim()||null,reanalysis_of:null,idempotency_key:`ui-intent-${operationId}`,step_budget:v.stepBudget,model_budget:{max_invocations:v.maxInvocations,max_total_tokens:v.maxTotalTokens,finalization_reserve_ms:v.finalizationReserveMs},context:{surface:'react-v1',human_owns_project:true}};
   const r=v.mode==='sync'?await client.runIntentSync(operationId,task):await client.runIntentAsync(operationId,task);
   setAnalysis(r.value.analysis_id); setActive(operationId); return r.value;
 },onSuccess:()=>{pendingLaunch.current=null;}});
 const waitingStep=plan.data?.steps?.find?.(s=>s.state==='WAITING_FOR_HUMAN');
 const respond=useMutation({mutationFn:async(v)=>{
   if(!active||!waitingStep)throw new Error('No waiting human step');
   const signature=JSON.stringify({active,step:waitingStep.step_id,decision:v.decision,text:v.responseText});
   if(pendingHumanResponse.current?.signature!==signature){
     pendingHumanResponse.current={signature,responseId:crypto.randomUUID()};
   }
   return (await client.respondAgentic(active,waitingStep.step_id,{response_id:pendingHumanResponse.current.responseId,decision:v.decision,response_text:v.responseText})).value;
 },onSuccess:async()=>{pendingHumanResponse.current=null;humanForm.reset({decision:'CONTINUE',responseText:''});await Promise.all([qc.invalidateQueries({queryKey:['agentic-plan']}),qc.invalidateQueries({queryKey:['operation']}),qc.invalidateQueries({queryKey:['dashboard']}),qc.invalidateQueries({queryKey:['analysis']})]);}});
 if(!client)return <p>Connect first.</p>;
 return <section><h2>Analyze this Situation</h2><p>Ask Niccolò to reason over the current Situation. The default path waits for the answer; execution details are optional.</p>
 {situationMissing&&<p className="error" role="alert"><strong>This prepared Situation has not been created yet.</strong> <Link to="/">Start the Situation first →</Link></p>}
 {situationUnavailable&&<p className="error" role="alert">Niccolò cannot load this Situation: {situation.error.message}</p>}
 {guide&&!guide.hasGroundingMaterial&&<p className="focus-only-note"><strong>Focus-only first pass.</strong> This is allowed. Niccolò will reason from the Situation focus without attached records or active human context. <Link to="/">Add material first</Link> if that would make the answer better grounded.</p>}
 {modelUnavailable&&<p className="error" role="alert">A reasoning model has not been configured. For the local MVP, run bash scripts/configure-local-model.sh, restart Niccolò, then return here.</p>}
 {readiness.error&&<p className="error" role="alert">Niccolò is not ready to analyze: {readiness.error.message}</p>}
 <RuntimeCapacityPanel diagnostic={resources.data} pending={resources.isPending} error={resources.error}/>
 <form onSubmit={handleSubmit(v=>run.mutate(v))}>
   <label>What are you trying to do?<select {...register('intent')}><option value="ABOUT_TO_ACT_OR_SAY">I am about to say or do something</option><option value="UNDERSTAND">I am trying to understand something</option><option value="FIGURE_OUT_WHAT_MATTERS">Help me figure out what matters</option></select></label>
   <label>Goal / question (optional)<textarea {...register('goal')} placeholder="Use your own words. Niccolò can work from the Situation even if you leave this blank."/></label>
   <details><summary>Advanced execution</summary><label>Delivery<select {...register('mode')}><option value="sync">Wait for the answer</option><option value="async">Return immediately and observe by PULL</option></select></label><label>Reasoning step budget<input aria-label="Step budget" type="number" {...register('stepBudget')}/></label><label>Maximum model calls<input aria-label="Maximum model calls" type="number" {...register('maxInvocations')}/></label><label>Maximum reported model tokens<input aria-label="Maximum model tokens" type="number" {...register('maxTotalTokens')}/></label><label>Finalization reserve (ms)<input aria-label="Finalization reserve" type="number" {...register('finalizationReserveMs')}/></label><small>The defaults are appropriate for ordinary use. These are hard resource limits, not a quality score. Niccolò preserves a provisional result when a model-call budget is exhausted.</small></details>
   <button disabled={run.isPending||modelUnavailable||readiness.isPending||readiness.isError||situation.isPending||situationMissing||situationUnavailable}>{run.isPending?'Analyzing…':'Analyze this Situation'}</button>
 </form>{Object.keys(errors).length>0&&<p className="error">Check the request fields.</p>}{run.error&&<p className="error">{run.error.message}</p>}
 {run.isPending
   ? <p role="status">Starting the new analysis. The preceding run's answer and controls are hidden until this request resolves.</p>
   : <AnswerPanel output={output.data} operation={observed.operation??run.data?.operation??null} error={output.error}/>}
 {!run.isPending&&(observed.operation??run.data?.operation)?.result?.completion_status==='DEGRADED_RESOURCE_PRESSURE'&&<article className="resource-degradation-note" role="status"><strong>Reasoning path reduced under critical runtime pressure.</strong><p>Niccolò deferred only reasoning steps explicitly classified as nonessential for this pass. The result remains <strong>provisional</strong>; resource pressure is not evidence quality, confidence, completeness or truth.</p>{(observed.operation??run.data?.operation)?.result?.resource_degraded_steps?.length>0&&<p><small>Deferred: {(observed.operation??run.data?.operation).result.resource_degraded_steps.map(step=>step.kind.replaceAll('_',' ').toLowerCase()).join(' · ')}</small></p>}</article>}
 {!run.isPending&&budget.data?.budget&&<details className="budget-status"><summary>Reasoning budget</summary><p><strong>{budget.data.budget.used_invocations}/{budget.data.budget.max_invocations}</strong> model call(s) used · <strong>{budget.data.budget.used_total_tokens}/{budget.data.budget.max_total_tokens}</strong> reported token(s) used.</p>{!budget.data.budget.token_usage_complete&&<p><small>Some provider calls did not report token usage; used tokens are a lower bound and remaining tokens are an upper bound.</small></p>}{budget.data.budget.stop_reasons?.length>0&&<p><strong>Model-call boundary:</strong> {budget.data.budget.stop_reasons.join(', ').replaceAll('_',' ').toLowerCase()}</p>}<small>Budget accounting limits resource use; it is not a quality, confidence, or truth score.</small></details>}
 {!run.isPending&&budget.error&&<p className="error">Reasoning budget unavailable: {budget.error.message}</p>}
 {!run.isPending&&waitingStep&&<form className="human-boundary" onSubmit={humanForm.handleSubmit(v=>respond.mutate(v))}><h3>Niccolò needs your input</h3><p>{waitingStep.label}</p><p><small>Reasoning pauses here until you respond. Your input is recorded; it is not inferred consent for any external action.</small></p><label>What should happen next?<select {...humanForm.register('decision')}><option value="CONTINUE">Continue with this understanding</option><option value="REPLAN">Reconsider the reasoning plan</option><option value="STOP">Stop this analysis</option></select></label><label>Your input<textarea aria-label="Human response" {...humanForm.register('responseText')} placeholder="Clarify, correct, or explain what you want Niccolò to consider."/></label><button disabled={respond.isPending}>{respond.isPending?'Sending…':'Send input'}</button>{humanForm.formState.errors.responseText&&<p className="error">A response is required to continue or replan, but not to stop.</p>}{respond.error&&<p className="error">{respond.error.message}</p>}</form>}
 {!run.isPending&&<details className="desktop-only technical-details"><summary>Technical execution details</summary><div className="inspector-grid"><article><h3>Operation</h3><p><small>Transport: {observed.phase}{observed.stale?' · stale':''}</small></p><pre>{JSON.stringify(observed.operation??run.data?.operation??null,null,2)}</pre></article><article><h3>Agentic plan</h3><pre>{JSON.stringify(plan.data??null,null,2)}</pre></article><article><h3>Operation events</h3>{observed.eventsTruncated&&<p><small>Showing the most recent 500 events. Earlier events remain in the server history.</small></p>}<pre>{JSON.stringify(observed.events??[],null,2)}</pre></article><article><h3>Anytime checkpoints</h3><p><small>Durable progress observations. A provisional checkpoint is not truth, proof or completeness.</small></p>{latestCheckpoint&&<button disabled={exportCheckpoint.isPending} onClick={()=>exportCheckpoint.mutate({checkpoint:latestCheckpoint})}>{exportCheckpoint.isPending?'Preparing checkpoint ZIP…':'Download latest checkpoint package'}</button>}{latestCheckpoint&&latestMilestone.data&&<button disabled={exportCheckpoint.isPending} onClick={()=>exportCheckpoint.mutate({checkpoint:latestCheckpoint,milestone:latestMilestone.data})}>{exportCheckpoint.isPending?'Preparing checkpoint ZIP…':'Download exact milestone package'}</button>}{exportCheckpoint.error&&<p className="error">Checkpoint export unavailable: {exportCheckpoint.error.message}</p>}{latestCheckpoint&&<section className="checkpoint-milestone"><h4>Meaningful milestone</h4>{latestMilestone.data?<><p><strong>{latestMilestone.data.milestone_kind.replaceAll('_',' ').toLowerCase()}</strong> — {latestMilestone.data.reason}</p><small>Declaring a milestone did not change this checkpoint's provisional/final status and did not publish it remotely.</small><CheckpointMilestonePublicationPanel operationId={active} checkpoint={latestCheckpoint} milestone={latestMilestone.data}/></>:<><p><small>Optionally record why this checkpoint is worth preserving. This is a historical label, not an epistemic upgrade.</small></p><label>Milestone kind<select value={milestoneKind} onChange={event=>setMilestoneKind(event.target.value)}><option value="FIRST_PASS">First completed pass</option><option value="TIMED_PARTIAL">Timed/budget partial</option><option value="HUMAN_ACKNOWLEDGED_REVIEW">Human-acknowledged review</option><option value="FINAL_SYNTHESIS">Final synthesis</option><option value="OTHER">Other meaningful checkpoint</option></select></label><label>Why it matters<textarea value={milestoneReason} maxLength={1000} onChange={event=>setMilestoneReason(event.target.value)} placeholder="Why is this checkpoint worth preserving?"/></label><button disabled={!milestoneReason.trim()||declareMilestone.isPending} onClick={()=>declareMilestone.mutate()}>{declareMilestone.isPending?'Recording milestone…':'Declare latest checkpoint a milestone'}</button>{declareMilestone.error&&<p className="error">Milestone declaration failed: {declareMilestone.error.message}</p>}</>}</section>}<pre>{JSON.stringify(checkpoints.data?.checkpoints??[],null,2)}</pre></article></div></details>}
 <small>Advanced delivery choices use the same durable server-side intent semantics. They do not change the meaning of the analysis.</small></section>;
}
