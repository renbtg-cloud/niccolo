import {useState} from 'react';
import {comparisonQuestions, inquiryCriteria} from '../state/inquiryComparison.js';

export function InquiryComparisonPanel({snapshot}) {
  const [selected,setSelected] = useState([]);
  const options = snapshot.nodes.filter(n=>n.kind==='QUESTION' && (snapshot.question_inquiries ?? []).some(q=>q.question_object_id===n.object_id));
  const visibleSelection = selected.filter(id=>options.some(q=>q.object_id===id));
  const columns = comparisonQuestions(snapshot,visibleSelection);
  const nodes = new Map(snapshot.nodes.map(n=>[n.object_id,n]));
  const relations = new Map(snapshot.relations.map(r=>[r.relation_id,r]));
  const linkRows = [['gaps','Addressed gaps'],['hypotheses','Tested hypotheses'],['presuppositions','Explicit presuppositions']];
  if (options.length < 2) return null;
  return <details open><summary>Compare candidate investigative questions</summary>
    <p>Choose up to three questions. No automatic ranking, truth score, execution or budget reallocation. A reported authorization is not permission to act.</p>
    <fieldset><legend>Questions to compare ({visibleSelection.length}/3)</legend>{options.map(q=><label key={q.object_id}><input type="checkbox" checked={visibleSelection.includes(q.object_id)} disabled={visibleSelection.length>=3 && !visibleSelection.includes(q.object_id)} onChange={e=>setSelected(e.target.checked?[...visibleSelection,q.object_id]:visibleSelection.filter(id=>id!==q.object_id))}/>{q.text}</label>)}</fieldset>
    {columns.length<2 ? <p>Select at least two questions.</p> : <div className="inquiry-comparison-scroll"><table className="inquiry-comparison"><caption>Attributed criteria and links; columns follow your selection</caption><thead><tr><th scope="col">Criterion or relationship</th>{columns.map(c=><th scope="col" key={c.question.object_id}>{c.question.text}</th>)}</tr></thead><tbody>
      {linkRows.map(([key,label])=><tr key={key}><th scope="row">{label}</th>{columns.map(c=><td key={c.question.object_id}>{c.links[key].length===0?'No explicit link recorded.':<ul>{c.links[key].map(link=><li key={link.relation_id}>{nodes.get(link.target_object_id)?.text ?? link.target_object_id}<p>{relations.get(link.relation_id)?.rationale}</p></li>)}</ul>}</td>)}</tr>)}
      {inquiryCriteria.map(([dimension,label],i)=><tr key={dimension}><th scope="row">{label}</th>{columns.map(c=>{const criterion=c.criteria[i];return <td key={c.question.object_id}>{criterion.projectionIncomplete&&<p>Incomplete assessment projection; no conclusion inferred.</p>}{!criterion.judgments.length&&!criterion.projectionIncomplete&&<p>No explicit assessment recorded.</p>}{criterion.judgments.length>0&&<ul>{criterion.judgments.map(a=><li key={a.event_id}><strong>{a.input.result}</strong> · confidence {a.input.confidence}<p>{a.input.criterion} · {a.input.context}</p><p>{a.input.rationale}</p><small>Recorded by {a.recorded_by} · {a.event_id}</small></li>)}</ul>}</td>;})}</tr>)}
    </tbody></table></div>}
  </details>;
}
