import {useRef,useState} from 'react';
import {comparisonQuestions, inquiryCriteria, inquiryDecisionRequest} from '../state/inquiryComparison.js';

export function InquiryComparisonPanel({snapshot,client,onChanged}) {
  const [selected,setSelected] = useState([]);
  const [draft,setDraft] = useState({question:'',disposition:'',rationale:''});
  const [basis,setBasis] = useState([]);
  const [saving,setSaving] = useState(false);
  const [error,setError] = useState('');
  const pending = useRef(null);
  const options = snapshot.nodes.filter(n=>n.kind==='QUESTION' && (snapshot.question_inquiries ?? []).some(q=>q.question_object_id===n.object_id));
  const visibleSelection = selected.filter(id=>options.some(q=>q.object_id===id));
  const columns = comparisonQuestions(snapshot,visibleSelection);
  const nodes = new Map(snapshot.nodes.map(n=>[n.object_id,n]));
  const relations = new Map(snapshot.relations.map(r=>[r.relation_id,r]));
  const assessments = new Map((snapshot.assessments ?? []).map(a=>[a.event_id,a]));
  const revised = new Set((snapshot.assessments ?? []).map(a=>a.input.revises_event_id).filter(Boolean));
  const linkRows = [['gaps','Addressed gaps'],['hypotheses','Tested hypotheses'],['presuppositions','Explicit presuppositions']];
  if (options.length === 0) return null;
  async function save(event) {
    event.preventDefault();
    if (saving) return;
    try {
      const body=inquiryDecisionRequest(columns,draft,basis);
      const key=JSON.stringify(body);
      if (pending.current?.key!==key) pending.current={key,id:crypto.randomUUID()};
      setSaving(true);setError('');
      await client.recordInquiryDecision(pending.current.id,body);
      pending.current=null;setDraft({question:'',disposition:'',rationale:''});setBasis([]);
      await onChanged();
    } catch(e) {setError(e.message);} finally {setSaving(false);}
  }
  return <details open><summary>Compare candidate investigative questions</summary>
    <p>Choose up to three questions. No automatic ranking, truth score, execution or budget reallocation. A reported authorization is not permission to act.</p>
    <fieldset disabled={saving}><legend>Questions to consider ({visibleSelection.length}/3)</legend>{options.map(q=><label key={q.object_id}><input type="checkbox" checked={visibleSelection.includes(q.object_id)} disabled={visibleSelection.length>=3 && !visibleSelection.includes(q.object_id)} onChange={e=>{setSelected(e.target.checked?[...visibleSelection,q.object_id]:visibleSelection.filter(id=>id!==q.object_id));setBasis([]);setDraft({...draft,question:''});}}/>{q.text}</label>)}</fieldset>
    {columns.length<2 ? <p>Select at least two questions.</p> : <div className="inquiry-comparison-scroll"><table className="inquiry-comparison"><caption>Attributed criteria and links; columns follow your selection</caption><thead><tr><th scope="col">Criterion or relationship</th>{columns.map(c=><th scope="col" key={c.question.object_id}>{c.question.text}</th>)}</tr></thead><tbody>
      {linkRows.map(([key,label])=><tr key={key}><th scope="row">{label}</th>{columns.map(c=><td key={c.question.object_id}>{c.links[key].length===0?'No explicit link recorded.':<ul>{c.links[key].map(link=><li key={link.relation_id}>{nodes.get(link.target_object_id)?.text ?? link.target_object_id}<p>{relations.get(link.relation_id)?.rationale}</p></li>)}</ul>}</td>)}</tr>)}
      {inquiryCriteria.map(([dimension,label],i)=><tr key={dimension}><th scope="row">{label}</th>{columns.map(c=>{const criterion=c.criteria[i];return <td key={c.question.object_id}>{criterion.projectionIncomplete&&<p>Incomplete assessment projection; no conclusion inferred.</p>}{!criterion.judgments.length&&!criterion.projectionIncomplete&&<p>No explicit assessment recorded.</p>}{criterion.judgments.length>0&&<ul>{criterion.judgments.map(a=><li key={a.event_id}><strong>{a.input.result}</strong> · confidence {a.input.confidence}<p>{a.input.criterion} · {a.input.context}</p><p>{a.input.rationale}</p><small>Recorded by {a.recorded_by} · {a.event_id}</small></li>)}</ul>}</td>;})}</tr>)}
    </tbody></table></div>}
    {columns.length>0&&<form onSubmit={save}><fieldset disabled={saving}><legend>Record your investigative decision</legend><p>This records your decision and explicitly selected assessment events. It does not send a question, execute an action, grant permission or close a gap.</p><label>Question<select value={draft.question} onChange={e=>setDraft({...draft,question:e.target.value})} required><option value="">Choose…</option>{columns.map(c=><option key={c.question.object_id} value={c.question.object_id}>{c.question.text}</option>)}</select></label><label>Disposition<select value={draft.disposition} onChange={e=>setDraft({...draft,disposition:e.target.value})} required><option value="">Choose…</option><option value="PURSUE_NEXT">Pursue next</option><option value="DEFER">Defer</option><option value="DO_NOT_PURSUE">Do not pursue</option></select></label><label>Rationale<textarea required maxLength={4096} value={draft.rationale} onChange={e=>setDraft({...draft,rationale:e.target.value})}/></label><fieldset><legend>Judgments explicitly consulted (optional)</legend>{columns.flatMap(c=>c.criteria.flatMap(k=>k.judgments.map(a=><label key={a.event_id}><input type="checkbox" checked={basis.includes(a.event_id)} disabled={basis.length>=64&&!basis.includes(a.event_id)} onChange={e=>setBasis(e.target.checked?[...basis,a.event_id]:basis.filter(id=>id!==a.event_id))}/>{c.question.text}: {k.label} — {a.input.result} · {a.input.criterion} · {a.input.context}</label>)))}</fieldset><button type="submit">Record decision only</button>{error&&<p role="alert">{error}</p>}</fieldset></form>}
    {(snapshot.question_decisions??[]).length>0&&<details open><summary>Recorded investigative decisions — history, not execution</summary><ul>{snapshot.question_decisions.map(d=><li key={d.decision_id}><strong>{d.input.disposition}: {nodes.get(d.input.question_id)?.text ?? d.input.question_id}</strong><p>{d.input.rationale}</p><small>Recorded by {d.recorded_by} · {d.recorded_at}</small><p>Considered: {d.input.considered_question_ids.map(id=>nodes.get(id)?.text??id).join(' · ')}</p>{d.input.assessment_event_ids.length===0?<p>No assessment events explicitly cited.</p>:<ul>{d.input.assessment_event_ids.map(id=>{const a=assessments.get(id);return <li key={id}>{a?`${a.input.dimension}: ${a.input.result} — ${a.input.rationale}`:'Assessment reference unavailable'} · {id}{revised.has(id)&&<strong> · subsequently revised; original basis retained</strong>}</li>;})}</ul>}</li>)}</ul></details>}
  </details>;
}
