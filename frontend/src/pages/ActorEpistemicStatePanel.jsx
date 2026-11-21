import {useRef,useState} from 'react';

export const stateDimensions=['EXPOSURE','ACCESS','PERCEPTION','ATTENTION','EXAMINATION','COMPREHENSION','RECALL','SUSPICION','INCLINATION','DOUBT','BELIEF','SUSPENDED_JUDGMENT','CLAIMED_KNOWLEDGE','PERCEIVED_KNOWLEDGE','EVIDENCE_SUPPORTED_KNOWLEDGE'];
const judgments=['INDICATED','NOT_INDICATED','UNKNOWN','CONTESTED'];
const empty={subject_actor_id:'',target_object_id:'',target_state_event_id:null,dimension:'',judgment:'',context:'',temporal_scope:'',rationale:'',basis_evidence_id:'',revises_event_id:null};
const label=value=>value.replaceAll('_',' ').toLowerCase();
export function stateRequest(draft,actors,nodes,evidence,events) {
  if(!actors.some(a=>a.actor_id===draft.subject_actor_id)||!!draft.target_object_id===!!draft.target_state_event_id)throw new Error('Select a visible actor and exactly one target.');
  if(draft.target_object_id&&!nodes.some(n=>n.object_id===draft.target_object_id)||draft.target_state_event_id&&!events.some(e=>e.event_id===draft.target_state_event_id))throw new Error('Select a visible epistemic object or exact recorded state event.');
  if(!stateDimensions.includes(draft.dimension)||!judgments.includes(draft.judgment))throw new Error('Choose an independent dimension and judgment.');
  for(const [field,max] of [['context',2048],['temporal_scope',2048],['rationale',4096]])if(!draft[field].trim()||draft[field].trim().length>max)throw new Error('Context, temporal scope and rationale are required.');
  if(draft.basis_evidence_id&&!evidence.some(e=>e.evidence_id===draft.basis_evidence_id))throw new Error('Select a visible evidence fragment.');
  if(draft.dimension==='EVIDENCE_SUPPORTED_KNOWLEDGE'&&draft.judgment==='INDICATED'&&!draft.basis_evidence_id)throw new Error('Cite the evidence used for this judgment.');
  const body={...draft,target_object_id:draft.target_object_id||null,target_state_event_id:draft.target_state_event_id||null,context:draft.context.trim(),temporal_scope:draft.temporal_scope.trim(),rationale:draft.rationale.trim(),basis_evidence_id:draft.basis_evidence_id||null};
  if(body.revises_event_id){const prior=events.find(e=>e.event_id===body.revises_event_id);if(!prior||events.some(e=>e.input.revises_event_id===prior.event_id)||['subject_actor_id','target_object_id','target_state_event_id','dimension','context','temporal_scope'].some(k=>(prior.input[k]??null)!==body[k]))throw new Error('Revision must preserve the current state identity.');}
  return body;
}

export function ActorEpistemicStatePanel({snapshot,actors,client,onChanged}) {
  const [draft,setDraft]=useState(empty);
  const [saving,setSaving]=useState(false);
  const [error,setError]=useState('');
  const pending=useRef(null);
  const nodes=snapshot.nodes??[], evidence=snapshot.evidence??[], events=snapshot.actor_epistemic_states??[];
  const revised=new Set(events.map(e=>e.input.revises_event_id));
  const actorById=new Map(actors.map(a=>[a.actor_id,a])),nodeById=new Map(nodes.map(n=>[n.object_id,n])),eventById=new Map(events.map(e=>[e.event_id,e]));
  const actorLabel=id=>actorById.get(id)?.display_label??id;
  const stateLabel=e=>`${actorLabel(e.input.subject_actor_id)} — ${label(e.input.dimension)}: ${label(e.input.judgment)} (${e.event_id})${revised.has(e.event_id)?' — historically revised':''}`;
  const targetLabel=input=>input.target_state_event_id?`State event: ${eventById.has(input.target_state_event_id)?stateLabel(eventById.get(input.target_state_event_id)):input.target_state_event_id}`:(nodeById.get(input.target_object_id)?.text??input.target_object_id);
  const change=(key,value)=>setDraft(d=>({...d,[key]:value}));
  const locked=!!draft.revises_event_id;
  async function save(e){e.preventDefault();if(saving)return;try{const body=stateRequest(draft,actors,nodes,evidence,events);const key=JSON.stringify(body);if(pending.current?.key!==key)pending.current={key,id:crypto.randomUUID()};setSaving(true);setError('');await client.recordActorEpistemicState(pending.current.id,body);pending.current=null;setDraft(empty);await onChanged();}catch(e){setError(e.message);}finally{setSaving(false);}}
  return <details><summary>Actor epistemic states — independent attributed judgments</summary><p>Exposure, access, attention, comprehension, belief and claimed knowledge are distinct. No state is inferred from another, and no recorded judgment establishes a fact about an actor’s mind. Only the original recorder can revise their judgment; other investigators can record a separate judgment.</p>
    {!actors.length&&<p>No registered actor available; no identity is inferred.</p>}
    <form onSubmit={save}><fieldset disabled={saving||!actors.length||!nodes.length}><legend>{locked?'Revise an exact attributed judgment':'Record an attributed judgment'}</legend>
      <label>Actor<select required disabled={locked} value={draft.subject_actor_id} onChange={e=>change('subject_actor_id',e.target.value)}><option value="">Choose…</option>{actors.map(a=><option key={a.actor_id} value={a.actor_id}>{a.display_label}</option>)}</select></label>
      <label>Epistemic object or exact prior state<select required disabled={locked} value={draft.target_state_event_id?`state:${draft.target_state_event_id}`:draft.target_object_id?`object:${draft.target_object_id}`:''} onChange={e=>{const [kind,id]=e.target.value.split(':');setDraft(d=>({...d,target_object_id:kind==='object'?id:null,target_state_event_id:kind==='state'?id:null}));}}><option value="">Choose…</option><optgroup label="Objects">{nodes.map(n=><option key={n.object_id} value={`object:${n.object_id}`}>{n.text}</option>)}</optgroup><optgroup label="Prior attributed state events">{events.map(e=><option key={e.event_id} value={`state:${e.event_id}`}>{stateLabel(e)}</option>)}</optgroup></select></label>
      <label>Dimension<select required disabled={locked} value={draft.dimension} onChange={e=>change('dimension',e.target.value)}><option value="">Choose…</option>{stateDimensions.map(d=><option key={d} value={d}>{label(d)}</option>)}</select></label>
      <label>Judgment<select required value={draft.judgment} onChange={e=>change('judgment',e.target.value)}><option value="">Choose…</option>{judgments.map(j=><option key={j} value={j}>{label(j)}</option>)}</select></label>
      <label>Context<input required disabled={locked} maxLength={2048} value={draft.context} onChange={e=>change('context',e.target.value)}/></label>
      <label>Temporal scope<input required disabled={locked} maxLength={2048} value={draft.temporal_scope} onChange={e=>change('temporal_scope',e.target.value)}/></label>
      <label>Rationale<textarea required maxLength={4096} value={draft.rationale} onChange={e=>change('rationale',e.target.value)}/></label>
      <label>Evidence fragment<select value={draft.basis_evidence_id} onChange={e=>change('basis_evidence_id',e.target.value)}><option value="">Not explicitly linked</option>{evidence.map(e=><option key={e.evidence_id} value={e.evidence_id}>{e.speaker_label}: {e.content?.slice(0,120)} ({e.evidence_id})</option>)}</select></label>
      <button type="submit">Record judgment only</button>{locked&&<button type="button" onClick={()=>setDraft(empty)}>Cancel revision</button>}{error&&<p role="alert">{error}</p>}
    </fieldset></form>
    <h4>Attributed state history</h4><p>A reference to another actor's state identifies the exact prior event; later revisions do not silently replace it or imply knowledge.</p>{!events.length?<p>No explicit state judgment recorded. Absence is not unknown or a negative judgment.</p>:<ul>{events.map(e=><li key={e.event_id}><strong>{actorLabel(e.input.subject_actor_id)} — {label(e.input.dimension)}: {label(e.input.judgment)}</strong><p>{targetLabel(e.input)} · {e.input.context} · {e.input.temporal_scope}</p><p>{e.input.rationale}</p><small>By {e.recorded_by} · {e.recorded_at} · {e.event_id} · basis {e.input.basis_evidence_id??'not explicitly linked'}</small>{revised.has(e.event_id)?<p>Historical — explicitly revised.</p>:<button type="button" disabled={saving} onClick={()=>setDraft({...e.input,basis_evidence_id:e.input.basis_evidence_id??'',revises_event_id:e.event_id})}>Propose revision of this judgment</button>}</li>)}</ul>}
  </details>;
}
