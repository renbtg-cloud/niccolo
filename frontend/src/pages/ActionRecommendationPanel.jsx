import { useRef, useState } from 'react';

export const recommendationKinds = ['PRESERVE_EVIDENCE', 'REVIEW_PRIVILEGES', 'SEPARATE_DUTIES', 'INDEPENDENT_REVIEW', 'VERIFY_PROVENANCE', 'GATHER_INFORMATION', 'DELAY', 'INACTION'];
export const reversibilities = ['REVERSIBLE', 'PARTIALLY_REVERSIBLE', 'IRREVERSIBLE', 'UNKNOWN'];
const textFields = [
  ['purpose', 'Purpose'], ['context', 'Circumstances and scope'], ['rationale', 'Rationale'],
  ['conditions', 'Conditions (including any separate authorization needed)'],
  ['expected_consequences', 'Expected consequences, not observed outcomes'],
  ['uncertainty', 'Uncertainty'], ['risk', 'Risk and possible adverse consequences'],
];
const label = code => code.replaceAll('_', ' ').toLowerCase();
const alternativeDraft = () => ({ kind: '', rationale: '', expected_consequences: '' });
const emptyDraft = () => ({
  kind: '', recipient_actor_id: '', ground_object_ids: [], reversibility: '',
  alternatives: [alternativeDraft()], revises_recommendation_id: null,
  ...Object.fromEntries(textFields.map(([key]) => [key, ''])),
});

function requiredText(value, field) {
  if (typeof value !== 'string') throw new Error(`${field} is required.`);
  const trimmed = value.replace(/^\p{White_Space}+|\p{White_Space}+$/gu, '');
  if (!trimmed || value.includes('\0') || new TextEncoder().encode(value).length > 4096) {
    throw new Error(`${field} must contain text within 4096 UTF-8 bytes.`);
  }
  return trimmed;
}

export function recommendationRequest(draft, actors, grounds, history) {
  if (!recommendationKinds.includes(draft.kind) || !reversibilities.includes(draft.reversibility)) {
    throw new Error('Choose a recommendation kind and reversibility.');
  }
  if (!actors.some(actor => actor.actor_id === draft.recipient_actor_id)) throw new Error('Choose a registered recipient.');
  const ids = draft.ground_object_ids;
  if (!Array.isArray(ids) || !ids.length || ids.length > 64 || new Set(ids).size !== ids.length || ids.some(id => !grounds.some(ground => ground.object_id === id))) {
    throw new Error('Choose 1–64 distinct visible grounding objects.');
  }
  if (!Array.isArray(draft.alternatives) || !draft.alternatives.length || draft.alternatives.length > 8) throw new Error('Compare 1–8 explicit alternatives.');
  const alternatives = draft.alternatives.map(alternative => {
    if (!recommendationKinds.includes(alternative.kind) || alternative.kind === draft.kind) throw new Error('Each alternative must have a different kind from the proposed recommendation.');
    return {
      kind: alternative.kind,
      rationale: requiredText(alternative.rationale, 'Alternative rationale'),
      expected_consequences: requiredText(alternative.expected_consequences, 'Alternative consequences'),
    };
  });
  const revision = draft.revises_recommendation_id || null;
  if (revision) {
    const prior = history.find(item => item.recommendation_id === revision);
    if (!prior || prior.input.recipient_actor_id !== draft.recipient_actor_id || history.some(item => item.input.revises_recommendation_id === revision)) {
      throw new Error('Choose an unrevised recommendation to the same recipient.');
    }
  }
  return {
    kind: draft.kind, recipient_actor_id: draft.recipient_actor_id,
    ground_object_ids: [...ids], alternatives, reversibility: draft.reversibility,
    revises_recommendation_id: revision,
    ...Object.fromEntries(textFields.map(([key, name]) => [key, requiredText(draft[key], name)])),
  };
}

export function ActionRecommendationPanel({ snapshot, actors, client, onChanged }) {
  const [draft, setDraft] = useState(emptyDraft);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState('');
  const pending = useRef(null);
  const history = snapshot.action_recommendations ?? [];
  const grounds = [...(snapshot.nodes ?? []), ...(snapshot.arguments ?? []).map(argument => ({ object_id: argument.argument_id, kind: 'ARGUMENT', text: argument.rationale }))];
  const revised = new Set(history.map(item => item.input.revises_recommendation_id));
  const recipientLabel = id => actors.find(actor => actor.actor_id === id)?.display_label ?? id;
  const groundLabel = id => grounds.find(ground => ground.object_id === id)?.text ?? id;
  const change = (field, value) => setDraft(current => ({ ...current, [field]: value }));
  const changeAlternative = (index, field, value) => setDraft(current => ({ ...current, alternatives: current.alternatives.map((alternative, i) => i === index ? { ...alternative, [field]: value } : alternative) }));
  const reset = () => { pending.current = null; setDraft(emptyDraft()); setError(''); };
  async function save(event) {
    event.preventDefault();
    if (saving) return;
    try {
      const body = recommendationRequest(draft, actors, grounds, history);
      const key = JSON.stringify([client.config.workspace, client.config.situation, body]);
      if (pending.current?.key !== key) pending.current = { key, id: crypto.randomUUID() };
      setSaving(true);
      setError('');
      await client.recordActionRecommendation(pending.current.id, body);
      reset();
      await onChanged();
    } catch (err) { setError(err.message); }
    finally { setSaving(false); }
  }
  return <details>
    <summary>Action recommendations — advisory records, not execution</summary>
    <p>Record a recommendation for human consideration, including inaction or delay. Recording does not make a decision, grant permission, send a message, create an execution plan or perform an action. Its expected consequences are not actual outcomes.</p>
    <form onSubmit={save}>
      <fieldset disabled={saving || !actors.length || !grounds.length}>
        <legend>{draft.revises_recommendation_id ? 'Revise recommendation (preserve original)' : 'Record an advisory recommendation'}</legend>
        <label>Recipient<select required disabled={!!draft.revises_recommendation_id} value={draft.recipient_actor_id} onChange={event => change('recipient_actor_id', event.target.value)}>
          <option value="">Choose…</option>{actors.map(actor => <option key={actor.actor_id} value={actor.actor_id}>{actor.display_label}</option>)}
        </select></label>
        <label>Recommendation kind<select required value={draft.kind} onChange={event => change('kind', event.target.value)}>
          <option value="">Choose…</option>{recommendationKinds.map(kind => <option key={kind} value={kind}>{label(kind)}</option>)}
        </select></label>
        {textFields.map(([field, title]) => <label key={field}>{title}<textarea required maxLength={4096} value={draft[field]} onChange={event => change(field, event.target.value)}/></label>)}
        <label>Reversibility<select required value={draft.reversibility} onChange={event => change('reversibility', event.target.value)}>
          <option value="">Choose…</option>{reversibilities.map(value => <option key={value} value={value}>{label(value)}</option>)}
        </select></label>
        <fieldset><legend>Explicit grounds ({draft.ground_object_ids.length}/64)</legend>
          {grounds.map(ground => <label key={ground.object_id}><input type="checkbox" checked={draft.ground_object_ids.includes(ground.object_id)} disabled={draft.ground_object_ids.length >= 64 && !draft.ground_object_ids.includes(ground.object_id)} onChange={event => setDraft(current => ({ ...current, ground_object_ids: event.target.checked ? [...current.ground_object_ids, ground.object_id] : current.ground_object_ids.filter(id => id !== ground.object_id) }))}/> {ground.kind}: {ground.text}</label>)}
        </fieldset>
        <fieldset><legend>Compare alternatives (including inaction, delay or further inquiry where relevant)</legend>
          {draft.alternatives.map((alternative, index) => <div key={index}>
            <label>Alternative {index + 1} kind<select required value={alternative.kind} onChange={event => changeAlternative(index, 'kind', event.target.value)}><option value="">Choose…</option>{recommendationKinds.filter(kind => kind !== draft.kind).map(kind => <option key={kind} value={kind}>{label(kind)}</option>)}</select></label>
            <label>Alternative rationale<textarea required maxLength={4096} value={alternative.rationale} onChange={event => changeAlternative(index, 'rationale', event.target.value)}/></label>
            <label>Alternative expected consequences<textarea required maxLength={4096} value={alternative.expected_consequences} onChange={event => changeAlternative(index, 'expected_consequences', event.target.value)}/></label>
            {draft.alternatives.length > 1 && <button type="button" onClick={() => setDraft(current => ({ ...current, alternatives: current.alternatives.filter((_, i) => i !== index) }))}>Remove alternative {index + 1}</button>}
          </div>)}
          <button type="button" disabled={draft.alternatives.length >= 8} onClick={() => setDraft(current => ({ ...current, alternatives: [...current.alternatives, alternativeDraft()] }))}>Add alternative</button>
        </fieldset>
        <button type="submit">Record recommendation only</button>
        {draft.revises_recommendation_id && <button type="button" onClick={reset}>Cancel revision</button>}
      </fieldset>
      {error && <p className="error" role="alert">{error}</p>}
      {(!actors.length || !grounds.length) && <p>Register an actor and author a visible grounding object before recording a recommendation.</p>}
    </form>
    <h4>Attributed recommendation history</h4>
    {history.length ? <ul>{history.map(item => <li key={item.recommendation_id}>
      <strong>{label(item.input.kind)} — to {recipientLabel(item.input.recipient_actor_id)}</strong>
      <p>{item.input.purpose}</p><p>Reversibility: {label(item.input.reversibility)}</p>
      <dl>{textFields.slice(1).map(([field, title]) => <div key={field}><dt>{title}</dt><dd>{item.input[field]}</dd></div>)}</dl>
      <p>Grounds: {item.input.ground_object_ids.map(groundLabel).join(' · ')}</p>
      <ul>{item.input.alternatives.map((alternative, index) => <li key={index}><strong>{label(alternative.kind)}</strong><p>{alternative.rationale}</p><p>Expected: {alternative.expected_consequences}</p></li>)}</ul>
      <small>Recorded by {item.recorded_by} · {item.recorded_at} · {item.recommendation_id}{item.input.revises_recommendation_id && ` · revises ${item.input.revises_recommendation_id}`}</small>
      {revised.has(item.recommendation_id) ? <p>Historical — explicitly revised; original retained.</p> : <button type="button" disabled={saving} onClick={() => { pending.current = null; setError(''); setDraft({ ...item.input, revises_recommendation_id: item.recommendation_id }); }}>Propose revision</button>}
    </li>)}</ul> : <p>No action recommendation recorded.</p>}
  </details>;
}
