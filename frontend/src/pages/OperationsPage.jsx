import { useMutation } from '@tanstack/react-query';
import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { z } from 'zod';
import { useClient } from '../hooks/useClient.js';
import { useUiStore } from '../stores/ui.js';
import { useOperation } from '../hooks/useOperation.js';
import { useOperationEvents } from '../hooks/useOperationEvents.js';
import { useAgenticPlan } from '../hooks/useAgenticPlan.js';

const operationSchema=z.object({
  mode:z.enum(['sync','async']),
  kind:z.enum(['BUILD_BRIEFING','BUILD_HANDOFF','AGENTIC_ANALYSIS','REANALYZE']),
  stepBudget:z.coerce.number().int().min(1).max(10000),
});
export function OperationsPage(){
 const client=useClient(); const analysis=useUiStore(s=>s.analysis); const active=useUiStore(s=>s.activeOperation); const setActive=useUiStore(s=>s.setActiveOperation);
 const op=useOperation(active); const events=useOperationEvents(active); const plan=useAgenticPlan(active);
 const {register,handleSubmit,formState:{errors}}=useForm({resolver:zodResolver(operationSchema),defaultValues:{mode:'sync',kind:'AGENTIC_ANALYSIS',stepBudget:32}});
 const run=useMutation({mutationFn:async(v)=>{const id=crypto.randomUUID();const task={kind:v.kind,target_analysis_id:analysis,idempotency_key:`ui-${id}`,input:v.kind==='REANALYZE'?{new_analysis_id:crypto.randomUUID(),goal:'Reanalyze with the current Situation state'}:{surface:'react-v1',intent:'user-owned-project'},step_budget:v.stepBudget};const r=v.mode==='sync'?await client.runSync(id,task):await client.runAsync(id,task);setActive(id);return r.value;}});
 if(!client)return <p>Connect first.</p>;
 return <section><h2>Operations</h2><form onSubmit={handleSubmit(v=>run.mutate(v))}><select {...register('mode')}><option value="sync">Synchronous</option><option value="async">Asynchronous + PULL</option></select><select {...register('kind')}><option>AGENTIC_ANALYSIS</option><option>BUILD_BRIEFING</option><option>BUILD_HANDOFF</option><option>REANALYZE</option></select><input aria-label="Step budget" type="number" {...register('stepBudget')}/><button disabled={!analysis||run.isPending}>Run</button></form>{Object.keys(errors).length>0&&<p className="error">Check operation fields.</p>}{run.error&&<p className="error">{run.error.message}</p>}<h3>Operation</h3><pre>{JSON.stringify(op.data??run.data??null,null,2)}</pre><div className="desktop-only inspector-grid"><article><h3>Agentic plan</h3><pre>{JSON.stringify(plan.data??null,null,2)}</pre></article><article><h3>Operation events</h3><pre>{JSON.stringify(events.data??null,null,2)}</pre></article></div><small>V1 uses ordinary HTTP request/response and PULL. Async and sync invoke the same server-side task semantics.</small></section>;
}
