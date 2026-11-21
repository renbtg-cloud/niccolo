import { useRef, useState } from 'react';

const stages = { AUTHORIZATION_REPORT: 'Reported authorization (not a grant)', EXISTING_PLAN_REPORT: 'Report of an existing plan', EXECUTION_OCCURRENCE: 'Execution occurrence', EVENT_OBSERVATION: 'Event observation' };
const characterizations = ['REPORTED', 'OBSERVED', 'INFERRED', 'CONTESTED'];
const causes = { PREDATED_INTERVENTION: 'Assessed to predate intervention', POSSIBLY_INDUCED: 'Possibly induced', POSSIBLY_INTENSIFIED: 'Possibly intensified', POSSIBLY_INDEPENDENT: 'Possibly independent', UNDETERMINED: 'Undetermined' };
const emptyRecord = () => ({ kind: '', linked_decision_id: null, characterization: '', description: '', event_temporal_scope: '', acquisition_temporal_scope: '', basis_evidence_id: null, revises_record_id: null });
const emptyAssessment = () => ({ execution_record_id: '', event_observation_record_id: '', assessment: '', rationale: '', alternative_explanations: '', uncertainty: '', basis_evidence_id: null, revises_assessment_id: null });
const text = (value, max) => {
  if (typeof value !== 'string' || !value.trim() || value.includes('\0') || new TextEncoder().encode(value).length > max) throw new Error(`Provide nonempty text within ${max} UTF-8 bytes.`);
  return value;
};
const revision = (items, id, field) => {
  if (!id) return null;
  const prior = items.find(item => item.id === id);
  if (!prior || items.some(item => item.input[field] === id)) throw new Error('Choose an unrevised record.');
  return prior;
};

export function interventionRequest(draft, records, decisions, evidence) {
  if (!Object.hasOwn(stages, draft.kind) || !characterizations.includes(draft.characterization)) throw new Error('Choose the stage and characterization.');
  if (draft.linked_decision_id && !decisions.some(d => d.decision_id === draft.linked_decision_id)) throw new Error('Choose a visible decision revision.');
  if (draft.characterization === 'OBSERVED' && !draft.basis_evidence_id) throw new Error('Observed characterization requires source evidence.');
  if (draft.basis_evidence_id && !evidence.some(e => e.evidence_id === draft.basis_evidence_id)) throw new Error('Choose visible source evidence.');
  const prior = revision(records, draft.revises_record_id, 'revises_record_id');
  if (prior && (prior.input.kind !== draft.kind || prior.input.linked_decision_id !== draft.linked_decision_id)) throw new Error('A revision preserves stage and decision pin.');
  return { kind: draft.kind, linked_decision_id: draft.linked_decision_id || null, characterization: draft.characterization, description: text(draft.description, 4096), event_temporal_scope: text(draft.event_temporal_scope, 2048), acquisition_temporal_scope: text(draft.acquisition_temporal_scope, 2048), basis_evidence_id: draft.basis_evidence_id || null, revises_record_id: draft.revises_record_id || null };
}

export function causalAssessmentRequest(draft, records, assessments, evidence) {
  if (!records.some(r => r.id === draft.execution_record_id && r.input.kind === 'EXECUTION_OCCURRENCE') || !records.some(r => r.id === draft.event_observation_record_id && r.input.kind === 'EVENT_OBSERVATION')) throw new Error('Pin an execution occurrence and a distinct event observation.');
  if (!Object.hasOwn(causes, draft.assessment)) throw new Error('Choose an explicit causal assessment.');
  if (draft.basis_evidence_id && !evidence.some(e => e.evidence_id === draft.basis_evidence_id)) throw new Error('Choose visible source evidence.');
  const prior = revision(assessments, draft.revises_assessment_id, 'revises_assessment_id');
  if (prior && (prior.input.execution_record_id !== draft.execution_record_id || prior.input.event_observation_record_id !== draft.event_observation_record_id)) throw new Error('A revision preserves both exact record pins.');
  return { execution_record_id: draft.execution_record_id, event_observation_record_id: draft.event_observation_record_id, assessment: draft.assessment, rationale: text(draft.rationale, 4096), alternative_explanations: text(draft.alternative_explanations, 4096), uncertainty: text(draft.uncertainty, 4096), basis_evidence_id: draft.basis_evidence_id || null, revises_assessment_id: draft.revises_assessment_id || null };
}

function useAuthoring(empty, client, onChanged) {
  const [draft, setDraft] = useState(empty);
  const [error, setError] = useState('');
  const [saving, setSaving] = useState(false);
  const pending = useRef(null);
  const reset = () => { pending.current = null; setDraft(empty()); setError(''); };
  const revise = draft => { pending.current = null; setError(''); setDraft(draft); };
  const change = (key, value) => setDraft(current => ({ ...current, [key]: value }));
  const save = async (event, build, send) => {
    event.preventDefault();
    if (saving) return;
    try {
      const body = build(draft);
      const key = JSON.stringify([client.config.workspace, client.config.situation, body]);
      if (pending.current?.key !== key) pending.current = { key, id: crypto.randomUUID() };
      setSaving(true); setError('');
      await send(pending.current.id, body); reset(); await onChanged();
    } catch (err) { setError(err.message); } finally { setSaving(false); }
  };
  return { draft, error, saving, reset, revise, change, save };
}

function TextField({ form, field, label, max = 4096 }) {
  return <label>{label}<textarea required maxLength={max} value={form.draft[field]} onChange={e => form.change(field, e.target.value)}/></label>;
}
function EvidenceField({ form, evidence, required = false }) {
  return <label>Source evidence {required ? '(required)' : '(optional)'}<select required={required} value={form.draft.basis_evidence_id ?? ''} onChange={e => form.change('basis_evidence_id', e.target.value || null)}><option value="">No source pinned</option>{evidence.map(e => <option key={e.evidence_id} value={e.evidence_id}>{e.document_locator} #{e.ordinal}: {e.content}</option>)}</select></label>;
}
function Attribution({ record, revisionField }) {
  return <small>Recorded by {record.recorded_by} · {record.recorded_at} · {record.id}{record.input[revisionField] && ` · revises ${record.input[revisionField]}`}</small>;
}

export function InterventionPanel({ snapshot, evidence = [], client, onChanged }) {
  const records = snapshot.intervention_records ?? [];
  const views = snapshot.intervention_causal_assessments ?? [];
  const assessments = views.map(view => view.assessment);
  const decisions = snapshot.action_decisions ?? [];
  const historical = new Set(records.map(r => r.input.revises_record_id));
  const recordForm = useAuthoring(emptyRecord, client, onChanged);
  const causalForm = useAuthoring(emptyAssessment, client, onChanged);
  const rd = recordForm.draft, cd = causalForm.draft;
  const recordLabel = id => { const row = records.find(r => r.id === id); return row ? `${row.input.description} · ${historical.has(id) ? 'historical' : 'current'} · ${id}` : id; };
  return <details>
    <summary>Intervention feedback — occurrence and causal uncertainty</summary>
    <p>Record reports and observations without granting authorization, devising plans or executing actions. A reported event, a decision and an assessment of its causal contribution are independent. Sequence alone does not establish causation.</p>
    <form onSubmit={e => recordForm.save(e, draft => interventionRequest(draft, records, decisions, evidence), (id, body) => client.recordIntervention(id, body))}>
      <fieldset disabled={recordForm.saving}><legend>{rd.revises_record_id ? 'Revise attributed record' : 'Record an intervention-related description'}</legend>
        <label>Stage<select required disabled={!!rd.revises_record_id} value={rd.kind} onChange={e => recordForm.change('kind', e.target.value)}><option value="">Choose…</option>{Object.entries(stages).map(([k, v]) => <option key={k} value={k}>{v}</option>)}</select></label>
        <label>Optional exact decision revision<select disabled={!!rd.revises_record_id} value={rd.linked_decision_id ?? ''} onChange={e => recordForm.change('linked_decision_id', e.target.value || null)}><option value="">No decision pinned</option>{decisions.map(d => <option key={d.decision_id} value={d.decision_id}>{d.input.rationale} · {d.decision_id}</option>)}</select></label>
        <label>Characterization<select required value={rd.characterization} onChange={e => recordForm.change('characterization', e.target.value)}><option value="">Choose…</option>{characterizations.map(c => <option key={c}>{c}</option>)}</select></label>
        <TextField form={recordForm} field="description" label="Description (not an instruction)"/>
        <TextField form={recordForm} field="event_temporal_scope" label="Event time or uncertainty" max={2048}/>
        <TextField form={recordForm} field="acquisition_temporal_scope" label="Information acquisition time or uncertainty" max={2048}/>
        <EvidenceField form={recordForm} evidence={evidence} required={rd.characterization === 'OBSERVED'}/>
        <button>Record description only</button>{rd.revises_record_id && <button type="button" onClick={recordForm.reset}>Cancel revision</button>}
      </fieldset>{recordForm.error && <p className="error" role="alert">{recordForm.error}</p>}
    </form>
    <h4>Attributed occurrence and report history</h4>
    {records.length ? <ul>{records.map(r => <li key={r.id}><strong>{stages[r.input.kind]} · {r.input.characterization}</strong><p>{r.input.description}</p><p>Event time: {r.input.event_temporal_scope} · Acquired: {r.input.acquisition_temporal_scope}</p><p>Decision pin: {r.input.linked_decision_id ?? 'none'} · Evidence pin: {r.input.basis_evidence_id ?? 'none'}</p><Attribution record={r} revisionField="revises_record_id"/>{historical.has(r.id) ? <p>Historical record — original retained.</p> : <button type="button" disabled={recordForm.saving} onClick={() => recordForm.revise({ ...r.input, revises_record_id: r.id })}>Propose record revision (original recorder only)</button>}</li>)}</ul> : <p>No intervention-related records.</p>}
    <form onSubmit={e => causalForm.save(e, draft => causalAssessmentRequest(draft, records, assessments, evidence), (id, body) => client.recordInterventionAssessment(id, body))}>
      <fieldset disabled={causalForm.saving}><legend>{cd.revises_assessment_id ? 'Revise assessment on exact historical pins' : 'Record separate causal assessment'}</legend>
        {[[ 'execution_record_id', 'Execution occurrence', 'EXECUTION_OCCURRENCE' ], [ 'event_observation_record_id', 'Event observation', 'EVENT_OBSERVATION' ]].map(([field, label, kind]) => <label key={field}>{label}<select required disabled={!!cd.revises_assessment_id} value={cd[field]} onChange={e => causalForm.change(field, e.target.value)}><option value="">Choose…</option>{records.filter(r => r.input.kind === kind).map(r => <option key={r.id} value={r.id}>{recordLabel(r.id)}</option>)}</select></label>)}
        <label>Causal assessment<select required value={cd.assessment} onChange={e => causalForm.change('assessment', e.target.value)}><option value="">Choose…</option>{Object.entries(causes).map(([k, v]) => <option key={k} value={k}>{v}</option>)}</select></label>
        <TextField form={causalForm} field="rationale" label="Rationale"/><TextField form={causalForm} field="alternative_explanations" label="Alternative explanations"/><TextField form={causalForm} field="uncertainty" label="Uncertainty and limitations"/>
        <EvidenceField form={causalForm} evidence={evidence}/><button>Record causal assessment only</button>{cd.revises_assessment_id && <button type="button" onClick={causalForm.reset}>Cancel revision</button>}
      </fieldset>{causalForm.error && <p className="error" role="alert">{causalForm.error}</p>}
    </form>
    <h4>Attributed causal assessment history</h4>
    {views.length ? <ul>{views.map(view => { const a = view.assessment; return <li key={a.id}><strong>{causes[a.input.assessment]}</strong><p>Execution pin: {recordLabel(a.input.execution_record_id)}</p><p>Observation pin: {recordLabel(a.input.event_observation_record_id)}</p><p>{a.input.rationale}</p><p>Alternatives: {a.input.alternative_explanations}</p><p>Uncertainty: {a.input.uncertainty}</p><p>Evidence pin: {a.input.basis_evidence_id ?? 'none'}</p><Attribution record={a} revisionField="revises_assessment_id"/>{view.requires_review && <p role="status">Pinned record revised; review required. Original assessment retained, not automatically invalidated. Record a new assessment to explicitly pin newer records.</p>}{view.superseded ? <p>Historical assessment — original retained.</p> : <button type="button" disabled={causalForm.saving} onClick={() => causalForm.revise({ ...a.input, revises_assessment_id: a.id })}>Propose assessment revision (original recorder only)</button>}</li>; })}</ul> : <p>No causal assessment recorded.</p>}
  </details>;
}
