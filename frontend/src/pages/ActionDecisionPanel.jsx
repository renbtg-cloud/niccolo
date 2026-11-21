import { useRef, useState } from 'react';

const dispositions = {
  SELECT_PRIMARY: 'Select primary recommendation',
  SELECT_ALTERNATIVE: 'Select an explicitly recorded alternative',
  DEFER: 'Defer the decision',
  DECLINE: 'Decline all proposed options',
};
const emptyDraft = () => ({ recommendation_id: '', disposition: '', alternative_index: null, rationale: '', revises_decision_id: null });
const kindLabel = kind => kind.replaceAll('_', ' ').toLowerCase();

export function decisionRequest(draft, recommendations, decisions) {
  const recommendation = recommendations.find(item => item.recommendation_id === draft.recommendation_id);
  if (!recommendation || !Object.hasOwn(dispositions, draft.disposition)) throw new Error('Choose an exact recommendation and a decision disposition.');
  const rationale = typeof draft.rationale === 'string' ? draft.rationale.replace(/^\p{White_Space}+|\p{White_Space}+$/gu, '') : '';
  if (!rationale || draft.rationale.includes('\0') || new TextEncoder().encode(draft.rationale).length > 4096) throw new Error('Supply a rationale within 4096 UTF-8 bytes.');
  const alternativeIndex = draft.alternative_index;
  if (draft.disposition === 'SELECT_ALTERNATIVE') {
    if (!Number.isInteger(alternativeIndex) || alternativeIndex < 0 || alternativeIndex >= recommendation.input.alternatives.length) throw new Error('Choose an alternative on this exact recommendation.');
  } else if (alternativeIndex !== null) {
    throw new Error('Only an alternative selection may carry an alternative index.');
  }
  const revision = draft.revises_decision_id || null;
  if (revision) {
    const prior = decisions.find(item => item.decision_id === revision);
    if (!prior || prior.input.recommendation_id !== draft.recommendation_id || decisions.some(item => item.input.revises_decision_id === revision)) throw new Error('Revise only an unrevised decision pinned to the same recommendation.');
  }
  return { recommendation_id: draft.recommendation_id, disposition: draft.disposition, alternative_index: alternativeIndex, rationale, revises_decision_id: revision };
}

export function decisionChoiceLabel(input, recommendation) {
  if (input.disposition === 'SELECT_PRIMARY') return `Primary: ${recommendation ? kindLabel(recommendation.input.kind) : 'pinned recommendation unavailable'}`;
  if (input.disposition === 'SELECT_ALTERNATIVE') {
    const alternative = recommendation?.input.alternatives[input.alternative_index];
    return `Alternative ${input.alternative_index + 1}: ${alternative ? kindLabel(alternative.kind) : 'pinned alternative unavailable'}`;
  }
  return dispositions[input.disposition] ?? input.disposition;
}

export function ActionDecisionPanel({ snapshot, client, onChanged }) {
  const [draft, setDraft] = useState(emptyDraft);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState('');
  const pending = useRef(null);
  const recommendations = snapshot.action_recommendations ?? [];
  const decisions = snapshot.action_decisions ?? [];
  const recommendationById = new Map(recommendations.map(item => [item.recommendation_id, item]));
  const recommendationSuccessors = new Map(recommendations.filter(item => item.input.revises_recommendation_id).map(item => [item.input.revises_recommendation_id, item.recommendation_id]));
  const revisedDecisions = new Set(decisions.map(item => item.input.revises_decision_id));
  const selected = recommendationById.get(draft.recommendation_id);
  const locked = !!draft.revises_decision_id;
  const change = (field, value) => setDraft(current => ({ ...current, [field]: value }));
  const reset = () => { pending.current = null; setDraft(emptyDraft()); setError(''); };
  async function save(event) {
    event.preventDefault();
    if (saving) return;
    try {
      const body = decisionRequest(draft, recommendations, decisions);
      const key = JSON.stringify([client.config.workspace, client.config.situation, body]);
      if (pending.current?.key !== key) pending.current = { key, id: crypto.randomUUID() };
      setSaving(true);
      setError('');
      await client.recordActionDecision(pending.current.id, body);
      reset();
      await onChanged();
    } catch (err) { setError(err.message); }
    finally { setSaving(false); }
  }
  return <details>
    <summary>Action decisions — separate from recommendations and authorization</summary>
    <p>Record a human choice among the explicitly proposed options, including a choice to defer or decline. A decision does not grant permission, produce an execution plan, perform an action or establish an outcome. Selecting inaction as an option is different from deferring the decision itself.</p>
    <form onSubmit={save}>
      <fieldset disabled={saving || !recommendations.length}>
        <legend>{locked ? 'Revise a decision on the same pinned recommendation' : 'Record a choice on an exact recommendation revision'}</legend>
        <label>Recommendation revision<select required disabled={locked} value={draft.recommendation_id} onChange={event => setDraft(current => ({ ...current, recommendation_id: event.target.value, alternative_index: null }))}>
          <option value="">Choose…</option>
          {recommendations.map(item => <option key={item.recommendation_id} value={item.recommendation_id}>{item.input.purpose} — {recommendationSuccessors.has(item.recommendation_id) ? 'historical' : 'current in its revision chain'} · {item.recommendation_id}</option>)}
        </select></label>
        {selected && <><p>Primary option: {kindLabel(selected.input.kind)} — {selected.input.purpose}</p><p>Conditions: {selected.input.conditions}</p><p>Uncertainty: {selected.input.uncertainty}</p><p>Risk: {selected.input.risk}</p></>}
        {selected && recommendationSuccessors.has(selected.recommendation_id) && <p>Historical recommendation explicitly selected. Choosing it does not select its successor.</p>}
        <label>Decision<select required value={draft.disposition} onChange={event => setDraft(current => ({ ...current, disposition: event.target.value, alternative_index: null }))}>
          <option value="">Choose…</option>{Object.entries(dispositions).map(([code, label]) => <option key={code} value={code}>{label}</option>)}
        </select></label>
        {draft.disposition === 'SELECT_ALTERNATIVE' && <label>Exact alternative<select required value={draft.alternative_index ?? ''} onChange={event => change('alternative_index', event.target.value === '' ? null : Number(event.target.value))}>
          <option value="">Choose…</option>{(selected?.input.alternatives ?? []).map((alternative, index) => <option key={index} value={index}>{index + 1}. {kindLabel(alternative.kind)} — {alternative.rationale}</option>)}
        </select></label>}
        {selected && draft.disposition === 'SELECT_ALTERNATIVE' && draft.alternative_index !== null && selected.input.alternatives[draft.alternative_index] && <p>Expected alternative consequences: {selected.input.alternatives[draft.alternative_index].expected_consequences}</p>}
        <label>Rationale<textarea required maxLength={4096} value={draft.rationale} onChange={event => change('rationale', event.target.value)}/></label>
        <button type="submit">Record action decision only</button>
        {locked && <button type="button" onClick={reset}>Cancel revision</button>}
      </fieldset>
      {error && <p className="error" role="alert">{error}</p>}
      {!recommendations.length && <p>Record a recommendation before recording a choice.</p>}
    </form>
    <h4>Attributed action decision history</h4>
    {decisions.length ? <ul>{decisions.map(item => {
      const recommendation = recommendationById.get(item.input.recommendation_id);
      const successor = recommendationSuccessors.get(item.input.recommendation_id);
      return <li key={item.decision_id}>
        <strong>{decisionChoiceLabel(item.input, recommendation)}</strong>
        <p>Pinned recommendation: {recommendation?.input.purpose ?? item.input.recommendation_id} · {item.input.recommendation_id}</p>
        {successor && <p>Recommendation subsequently revised; original choice retained, not rebound to successor {successor}.</p>}
        <p>{item.input.rationale}</p><small>Recorded by {item.recorded_by} · {item.recorded_at} · {item.decision_id}{item.input.revises_decision_id && ` · revises ${item.input.revises_decision_id}`}</small>
        {revisedDecisions.has(item.decision_id) ? <p>Historical decision — explicitly revised; original retained.</p> : <button type="button" disabled={saving} onClick={() => { pending.current = null; setError(''); setDraft({ ...item.input, revises_decision_id: item.decision_id }); }}>Propose decision revision (original recorder only)</button>}
      </li>;
    })}</ul> : <p>No action decision recorded.</p>}
  </details>;
}
