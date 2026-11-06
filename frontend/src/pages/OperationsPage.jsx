import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { z } from 'zod';
import { useClient } from '../hooks/useClient.js';
import { useUiStore } from '../stores/ui.js';
import { useOperationObserver } from '../hooks/useOperationObserver.js';
import { useAgenticPlan } from '../hooks/useAgenticPlan.js';

const operationSchema=z.object({
  mode:z.enum(['sync','async']),
  kind:z.enum(['BUILD_BRIEFING','BUILD_HANDOFF','AGENTIC_ANALYSIS','REANALYZE']),
  stepBudget:z.coerce.number().int().min(1).max(10000),
});
const humanSchema=z.object({decision:z.enum(['CONTINUE','STOP','REPLAN']),responseText:z.string().trim().min(1).max(4096)});
export function OperationsPage(){
 const client=useClient(); const qc=useQueryClient(); const analysis=useUiStore(s=>s.analysis); const active=useUiStore(s=>s.activeOperation); const setActive=useUiStore(s=>s.setActiveOperation);
 const observed=useOperationObserver(active); const plan=useAgenticPlan(active);
 const {register,handleSubmit,formState:{errors}}=useForm({resolver:zodResolver(operationSchema),defaultValues:{mode:'sync',kind:'AGENTIC_ANALYSIS',stepBudget:32}});
 const humanForm=useForm({resolver:zodResolver(humanSchema),defaultValues:{decision:'CONTINUE',responseText:''}});
 const run=useMutation({mutationFn:async(v)=>{const id=crypto.randomUUID();const task={kind:v.kind,target_analysis_id:analysis,idempotency_key:`ui-${id}`,input:v.kind==='REANALYZE'?{new_analysis_id:crypto.randomUUID(),goal:'Reanalyze with the current Situation state'}:{surface:'react-v1',intent:'user-owned-project'},step_budget:v.stepBudget};const r=v.mode==='sync'?await client.runSync(id,task):await client.runAsync(id,task);setActive(id);return r.value;}});
 const waitingStep=plan.data?.steps?.find?.(s=>s.state==='WAITING_FOR_HUMAN');
 const respond=useMutation({mutationFn:async(v)=>{if(!active||!waitingStep)throw new Error('No waiting human step');return (await client.respondAgentic(active,waitingStep.step_id,{response_id:crypto.randomUUID(),decision:v.decision,response_text:v.responseText})).value;},onSuccess:async()=>{humanForm.reset({decision:'CONTINUE',responseText:''});await Promise.all([qc.invalidateQueries({queryKey:['agentic-plan']}),qc.invalidateQueries({queryKey:['operation']}),qc.invalidateQueries({queryKey:['dashboard']})]);}});
 if(!client)return <p>Connect first.</p>;
 return <section><h2>Operations</h2><form onSubmit={handleSubmit(v=>run.mutate(v))}><select {...register('mode')}><option value="sync">Synchronous</option><option value="async">Asynchronous + PULL</option></select><select {...register('kind')}><option>AGENTIC_ANALYSIS</option><option>BUILD_BRIEFING</option><option>BUILD_HANDOFF</option><option>REANALYZE</option></select><input aria-label="Step budget" type="number" {...register('stepBudget')}/><button disabled={!analysis||run.isPending}>Run</button></form>{Object.keys(errors).length>0&&<p className="error">Check operation fields.</p>}{run.error&&<p className="error">{run.error.message}</p>}<h3>Operation</h3><p><small>Transport: {observed.phase}{observed.stale?' · stale':''}</small></p><pre>{JSON.stringify(observed.operation??run.data??null,null,2)}</pre>{waitingStep&&<form className="human-boundary" onSubmit={humanForm.handleSubmit(v=>respond.mutate(v))}><h3>Human boundary</h3><p>{waitingStep.label}</p><select {...humanForm.register('decision')}><option value="CONTINUE">Continue provisionally</option><option value="REPLAN">Replan from current state</option><option value="STOP">Stop</option></select><textarea aria-label="Human response" {...humanForm.register('responseText')} placeholder="Your decision / correction / context"/><button disabled={respond.isPending}>Submit human decision</button>{humanForm.formState.errors.responseText&&<p className="error">Response is required.</p>}{respond.error&&<p className="error">{respond.error.message}</p>}</form>}<div className="desktop-only inspector-grid"><article><h3>Agentic plan</h3><pre>{JSON.stringify(plan.data??null,null,2)}</pre></article><article><h3>Operation events</h3><pre>{JSON.stringify(observed.events??[],null,2)}</pre></article></div><small>V1 uses ordinary HTTP request/response and PULL. Async and sync invoke the same server-side task semantics.</small></section>;
}
