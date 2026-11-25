import { useState } from 'react';
import { useMutation } from '@tanstack/react-query';
import { relationEndpointAllowed, relationKinds } from '../state/expertWorkbench.js';
import { DraftGraphPreview } from './WorkbenchGraph.jsx';

const draftKinds = {
  PROPOSITION: 'Statement',
  HYPOTHESIS: 'Hypothesis',
  ASSUMPTION: 'What-if assumption',
  INFORMATION_GAP: 'Information gap',
  QUESTION: 'Investigative question',
};
const inputLimit = 4096;
const encoder = new TextEncoder();
const draftRelations = [...relationKinds.SEMANTIC, ...relationKinds.CONTEXTUAL];

export function removeDraftObject(graph, index) {
  const remap = (endpoint) => endpoint > index ? endpoint - 1 : endpoint;
  return {
    objects: graph.objects.filter((_, position) => position !== index),
    relations: graph.relations
      .filter(({ from, to }) => from !== index && to !== index)
      .map((relation) => ({ ...relation, from: remap(relation.from), to: remap(relation.to) })),
  };
}

function validDraftRelation(relation, objects) {
  const from = objects[relation.from]?.kind;
  const to = objects[relation.to]?.kind;
  const family = relationKinds.SEMANTIC.includes(relation.kind) ? 'SEMANTIC' : 'CONTEXTUAL';
  return from && to && relation.from !== relation.to
    && relationEndpointAllowed(family, relation.kind, from, to)
    && relation.explanation.trim() && encoder.encode(relation.explanation).length <= 1024;
}

export function InvestigatorInputDraftPanel({ client, enabled, onAdopted }) {
  const [text, setText] = useState('');
  const [suggestion, setSuggestion] = useState(null);
  const [reviewed, setReviewed] = useState(null);
  const [saved, setSaved] = useState(false);
  const generate = useMutation({
    mutationFn: async (input) => (await client.draftExpertInput(input)).value,
    onSuccess: (result) => {
      setSuggestion(result);
      setReviewed(result.draft);
    },
  });
  const adopt = useMutation({
    mutationFn: ({ id, graph }) => client.adoptExpertInputDraft(id, graph),
    onSuccess: async () => {
      setSuggestion(null);
      setText('');
      setSaved(true);
      await onAdopted();
    },
  });
  const canDraft = text.trim() && encoder.encode(text).length <= inputLimit;
  const pending = generate.isPending || adopt.isPending;
  const canAdopt = reviewed?.objects.length > 0
    && reviewed.objects.every((object) => object.text.trim() && encoder.encode(object.text).length <= 2048)
    && reviewed.relations.every((relation) => validDraftRelation(relation, reviewed.objects));
  function submit(event) {
    event.preventDefault();
    if (!canDraft || pending) return;
    setSuggestion(null);
    setSaved(false);
    adopt.reset();
    generate.mutate(text);
  }
  const update = (items, index, patch) => setReviewed((graph) => ({
    ...graph, [items]: graph[items].map((item, position) => index === position ? { ...item, ...patch } : item),
  }));
  return <article className="review-panel">
    <h3>Draft from investigator text</h3>
    <p>Ask the configured model to propose statements and connections from this text. Recording the reviewed structure keeps the input and the original proposal for provenance.</p>
    {enabled ? <form onSubmit={submit} className="human-context">
      <label>Investigator text<textarea value={text} maxLength={inputLimit} disabled={pending} onChange={event => setText(event.target.value)}/></label>
      <button type="submit" disabled={!canDraft || pending}>Propose a draft</button>
    </form> : <p>Configure a model to draft from text. Manual authoring is available below.</p>}
    {generate.isPending && <p role="status">Waiting for the model's draft…</p>}
    {generate.error && <p role="alert" className="error">{generate.error.message}</p>}
    {saved && <p role="status">Reviewed structure recorded in the Workbench.</p>}
    {suggestion && reviewed && <>
      <h4>Proposed structure for review</h4>
      <p>Suggested by {suggestion.provider} / {suggestion.model}. Edit or remove any item before recording.</p>
      <blockquote>{suggestion.input_text}</blockquote>
      <DraftGraphPreview graph={reviewed}/>
      {!reviewed.objects.length && <p>No objects to record in this draft.</p>}
      <fieldset disabled={pending} className="human-context">
        <legend>Objects and connections</legend>
        {reviewed.objects.map((item, index) => <div className="context-item" key={index}>
          <label>Object {index + 1} kind<select value={item.kind} onChange={(event) => update('objects', index, { kind: event.target.value })}>
            {Object.entries(draftKinds).map(([kind, label]) => <option value={kind} key={kind}>{label}</option>)}
          </select></label>
          <label>Object {index + 1} text<textarea value={item.text} maxLength={2048} onChange={(event) => update('objects', index, { text: event.target.value })}/></label>
          <blockquote>{item.source_excerpt}</blockquote>
          <button type="button" onClick={() => setReviewed((graph) => removeDraftObject(graph, index))}>Remove object and its connections</button>
        </div>)}
        {reviewed.relations.map((relation, index) => <div className="context-item" key={index}>
          <div className="row">
            <label>From<select value={relation.from} onChange={(event) => update('relations', index, { from: Number(event.target.value) })}>
              {reviewed.objects.map((_, object) => <option value={object} key={object}>Object {object + 1}</option>)}
            </select></label>
            <label>Relationship<select value={relation.kind} onChange={(event) => update('relations', index, { kind: event.target.value })}>
              {draftRelations.map((kind) => <option value={kind} key={kind}>{kind.toLowerCase().replaceAll('_', ' ')}</option>)}
            </select></label>
            <span aria-hidden="true">→</span>
            <label>To<select value={relation.to} onChange={(event) => update('relations', index, { to: Number(event.target.value) })}>
              {reviewed.objects.map((_, object) => <option value={object} key={object}>Object {object + 1}</option>)}
            </select></label>
          </div>
          <label>Why these are connected<textarea value={relation.explanation} maxLength={1024} onChange={(event) => update('relations', index, { explanation: event.target.value })}/></label>
          {!validDraftRelation(relation, reviewed.objects) && <small>Choose two compatible objects and explain their connection.</small>}
          <button type="button" onClick={() => setReviewed((graph) => ({ ...graph, relations: graph.relations.filter((_, position) => position !== index) }))}>Remove connection</button>
        </div>)}
        <button type="button" disabled={!canAdopt || pending} onClick={() => adopt.mutate({ id: suggestion.draft_id, graph: reviewed })}>Record reviewed structure</button>
        <button type="button" onClick={() => { adopt.reset(); setSuggestion(null); }}>Close without recording</button>
      </fieldset>
      {adopt.error && <p role="alert" className="error">{adopt.error.message}</p>}
    </>}
  </article>;
}

export function DraftProvenance({ client, source }) {
  const read = useMutation({
    mutationFn: async () => (await client.expertInputDraft(source.draft_id)).value,
  });
  return <details onToggle={(event) => {
    if (event.currentTarget.open && read.isIdle) read.mutate();
  }}>
    <summary>Draft by {source.provider} / {source.model}</summary>
    {read.isPending && <p role="status">Loading draft provenance…</p>}
    {read.error && <button type="button" onClick={() => read.mutate()}>Retry loading draft provenance</button>}
    {read.data && <>
      <p>Original investigator text</p>
      <blockquote>{read.data.input_text}</blockquote>
      <p>Original model proposal</p>
      <ol>{read.data.draft.objects.map((object, index) => <li key={index}>{object.text}</li>)}</ol>
      {read.data.draft.relations.length > 0 && <ul>{read.data.draft.relations.map((relation, index) => <li key={index}>
        Object {relation.from + 1} → Object {relation.to + 1} · {relation.kind} · {relation.explanation}
      </li>)}</ul>}
      <p>Recorded by {read.data.adoption.adopted_by}</p>
      <ol>{read.data.adoption.reviewed_graph.objects.map((object, index) => <li key={index}>{object.text}</li>)}</ol>
    </>}
  </details>;
}
