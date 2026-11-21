import { useMemo, useState } from 'react';
import { useMutation, useQuery } from '@tanstack/react-query';
import { useClient } from '../hooks/useClient.js';
import {
  argumentModes,
  argumentRequest,
  applicabilityAssessments,
  applicabilityRequest,
  assessmentResults, assessmentConfidences, assessmentKey, assessmentRequest, assessmentTargetRevision,
  assessmentDependencyRequest, assessmentDependencyReviewRequest, assessmentDownstreamReviewRequest,
  groundingAdequacies,
  groundingEffects,
  groundingRequest,
  nodeKinds,
  nodeRequest,
  relationKinds,
  relationRequest,
  relationSemantics,
  reassessmentDispositions,
  reassessmentReviewRequest,
} from '../state/expertWorkbench.js';

function useWorkbenchMutation(client, workbenchQuery, fn) {
  return useMutation({
    mutationFn: fn,
    onSuccess: async () => workbenchQuery.refetch(),
  });
}

export function ExpertWorkbenchPage() {
  const client = useClient();
  const workbench = useQuery({
    queryKey: ['expert-epistemic-workbench', client?.config.workspace, client?.config.situation],
    enabled: !!client,
    queryFn: async () => (await client.expertWorkbench()).value,
  });
  const data = workbench.data ?? { nodes: [], evidence: [], evidence_truncated: false, relations: [], arguments: [], grounding_sets: [], reassessment_impacts: [], grounding_cycles: [], reasoning_diagnostics: [] };
  const [error, setError] = useState('');
  const [assessment, setAssessment] = useState({ target: '', dimension: 'CREDIBILITY', result: 'UNKNOWN', confidence: 'UNKNOWN', criterion: '', context: '', rationale: '', grounds: [] });
  const [dependency, setDependency] = useState({route: '', assessment: '', rationale: ''});
  const [dependencyReviews, setDependencyReviews] = useState({});
  const [node, setNode] = useState({ kind: 'HYPOTHESIS', text: '', rationale: '' });
  const [relation, setRelation] = useState({ family: 'SEMANTIC', kind: 'CONTRADICTS', left: '', right: '', rationale: '' });
  const [argument, setArgument] = useState({ conclusion: '', mode: 'ABDUCTIVE', rationale: '' });
  const [grounding, setGrounding] = useState({ argument: '', effect: 'SUPPORTS', adequacy: 'PARTIAL', rationale: '' });
  const [selectedGroundIds, setSelectedGroundIds] = useState([]);
  const [selectedEvidenceIds, setSelectedEvidenceIds] = useState([]);
  const [evidenceSearchText, setEvidenceSearchText] = useState('');
  const [groundRoles, setGroundRoles] = useState({});
  const [groundMemberRationales, setGroundMemberRationales] = useState({});
  const [reassessmentDrafts, setReassessmentDrafts] = useState({});
  const [applicability, setApplicability] = useState({route: '', condition: '', assessment: 'UNASSESSED', rationale: ''});

  const searchEvidence = useMutation({ mutationFn: async (query) => (await client.searchExpertEvidence(query, 20)).value });
  const createNode = useWorkbenchMutation(client, workbench, async (body) => client.createExpertNode(crypto.randomUUID(), body));
  const createRelation = useWorkbenchMutation(client, workbench, async (body) => client.createExpertRelation(crypto.randomUUID(), body));
  const createArgument = useWorkbenchMutation(client, workbench, async (body) => client.createExpertArgument(crypto.randomUUID(), body));
  const createGrounding = useWorkbenchMutation(client, workbench, async ({ argumentId, body }) => client.createExpertGrounding(argumentId, crypto.randomUUID(), body));
  const reviewReassessment = useWorkbenchMutation(client, workbench, async ({ impact, body }) => client.reviewExpertReassessment(impact.trigger_relation_id, impact.argument_id, crypto.randomUUID(), body));
  const recordApplicability = useWorkbenchMutation(client, workbench, async ({route, body}) => client.recordExpertApplicability(route.argument_id, route.grounding_set_id, crypto.randomUUID(), body));
  const recordAssessment = useWorkbenchMutation(client, workbench, async (body) => client.recordExpertAssessment(crypto.randomUUID(), body));
  const recordDependency = useWorkbenchMutation(client, workbench, async (body) => client.recordExpertAssessmentDependency(crypto.randomUUID(), body));
  const reviewDependency = useWorkbenchMutation(client, workbench, async ({id, body}) => client.reviewExpertAssessmentDependency(id, crypto.randomUUID(), body));
  const reviewDownstream = useWorkbenchMutation(client, workbench, async ({id, body}) => client.reviewExpertAssessmentDownstream(id, crypto.randomUUID(), body));
  const mutations = [createNode, createRelation, createArgument, createGrounding, reviewReassessment, recordApplicability, recordAssessment, recordDependency, reviewDependency, reviewDownstream];
  const mutationError = mutations.find((m) => m.error)?.error;
  const busy = mutations.some((m) => m.isPending);

  const selectedGroundNodes = useMemo(
    () => data.nodes.filter((n) => selectedGroundIds.includes(n.object_id)),
    [data.nodes, selectedGroundIds],
  );
  const evidenceChoices = useMemo(() => {
    const merged = new Map(data.evidence.map((item) => [item.evidence_id, item]));
    for (const item of searchEvidence.data?.results ?? []) merged.set(item.evidence_id, item);
    return [...merged.values()];
  }, [data.evidence, searchEvidence.data]);
  const selectedGroundEvidence = useMemo(
    () => evidenceChoices.filter((item) => selectedEvidenceIds.includes(item.evidence_id)),
    [evidenceChoices, selectedEvidenceIds],
  );
  const assessmentHistory = data.assessments ?? [];
  const assessmentChoices = [
    ...data.nodes.map(n => ({ target: {kind: 'OBJECT', object_id: n.object_id}, label: `${n.kind}: ${n.text}` })),
    ...data.arguments.map(a => ({target: {kind: 'OBJECT', object_id: a.argument_id}, label: `ARGUMENT: ${a.rationale}`})),
    ...evidenceChoices.map(e => ({target: {kind: 'EVIDENCE', evidence_id: e.evidence_id}, label: `EVIDENCE: ${e.document_locator} #${e.ordinal}: ${e.content}`})),
    ...data.relations.map(r => ({target: {kind: 'RELATION', relation_id: r.relation_id}, label: `${r.kind}: ${r.rationale}`})),
    ...data.grounding_sets.map(g => ({target: {kind: 'ROUTE', argument_id: g.argument_id, grounding_set_id: g.grounding_set_id}, label: `ROUTE ${g.effect}: ${g.rationale}`})),
    ...assessmentHistory.map(a => ({target: {kind:'ASSESSMENT',assessment_event_id:a.event_id}, label:`ASSESSMENT ${a.input.dimension}: ${a.input.result} — ${a.input.criterion} (${a.event_id})`})),
  ];
  const assessmentDependencies = data.assessment_dependencies ?? [];
  const routeLabel = route => data.grounding_sets.find(g => g.argument_id === route.argument_id && g.grounding_set_id === route.grounding_set_id)?.rationale ?? `${route.argument_id}/${route.grounding_set_id}`;
  const supersededAssessments = new Set(assessmentHistory.map(a => a.input.revises_event_id).filter(Boolean));
  const assessmentLabel = target => {
    const label = assessmentChoices.find(c => assessmentKey(c.target) === assessmentKey(target))?.label ?? assessmentKey(target);
    const successor = assessmentTargetRevision(target, assessmentHistory);
    return successor ? `${label} — pinned historical event; latest explicit revision ${successor}. This judgment does not assess the successor.` : label;
  };
  const submit = (action) => (event) => {
    event.preventDefault();
    setError('');
    try { action(); } catch (e) { setError(e.message); }
  };

  if (!client) return <section><h2>Epistemic Workbench</h2><p>Connect a Situation first.</p></section>;

  return <section className="expert-workbench">
    <h2>Epistemic Workbench</h2>
    <p>This MVP assumes a trained investigator who explicitly authors epistemic abstractions. Niccolò validates and reasons over the structure; it does not translate investigator free text into ontology.</p>
    <p className="epistemic-boundary"><strong>Post-MVP:</strong> optional free-text interpretation may propose this structure later. It is not enabled here.</p>
    {workbench.isPending && <p>Loading structured model…</p>}
    {workbench.error && <p className="error">Workbench unavailable: {workbench.error.message}</p>}
    {error && <p className="error" role="alert">{error}</p>}
    {mutationError && <p className="error" role="alert">Write failed: {mutationError.message}</p>}

    <div className="expert-workbench-grid">
      <form onSubmit={submit(() => createNode.mutate(nodeRequest(node.kind, node.text, node.rationale)))}>
        <h3>Add abstraction</h3>
        <label>Kind<select value={node.kind} onChange={(e) => setNode({ ...node, kind: e.target.value })}>{nodeKinds.map((x) => <option key={x}>{x}</option>)}</select></label>
        <label>Structured statement<textarea value={node.text} maxLength={8192} onChange={(e) => setNode({ ...node, text: e.target.value })} /></label>
        <label>Why this abstraction is appropriate<textarea value={node.rationale} maxLength={4096} onChange={(e) => setNode({ ...node, rationale: e.target.value })} /></label>
        <button disabled={busy}>Create object</button>
      </form>

      <form onSubmit={submit(() => createRelation.mutate(relationRequest(relation.family, relation.kind, relation.left, relation.right, relation.rationale)))}>
        <h3>Add typed relation</h3>
        <label>Family<select value={relation.family} onChange={(e) => setRelation({ ...relation, family: e.target.value, kind: relationKinds[e.target.value][0] })}>{Object.keys(relationKinds).map((x) => <option key={x}>{x}</option>)}</select></label>
        <label>Relation<select value={relation.kind} onChange={(e) => setRelation({ ...relation, kind: e.target.value })}>{relationKinds[relation.family].map((x) => <option key={x}>{x}</option>)}</select></label>
        <small className="relation-semantics">{relationSemantics[relation.kind].description}</small>
        <label>{relationSemantics[relation.kind].left}<select value={relation.left} onChange={(e) => setRelation({ ...relation, left: e.target.value })}><option value="">Choose…</option>{data.nodes.map((n) => <option key={n.object_id} value={n.object_id}>{n.kind}: {n.text}</option>)}</select></label>
        <label>{relationSemantics[relation.kind].right}<select value={relation.right} onChange={(e) => setRelation({ ...relation, right: e.target.value })}><option value="">Choose…</option>{data.nodes.map((n) => <option key={n.object_id} value={n.object_id}>{n.kind}: {n.text}</option>)}</select></label>
        <label>Rationale<textarea value={relation.rationale} maxLength={4096} onChange={(e) => setRelation({ ...relation, rationale: e.target.value })} /></label>
        <button disabled={busy}>Create relation</button>
      </form>

      <form onSubmit={submit(() => createArgument.mutate(argumentRequest(argument.conclusion, argument.mode, argument.rationale)))}>
        <h3>Add argument</h3>
        <label>Conclusion<select value={argument.conclusion} onChange={(e) => setArgument({ ...argument, conclusion: e.target.value })}><option value="">Choose…</option>{data.nodes.map((n) => <option key={n.object_id} value={n.object_id}>{n.kind}: {n.text}</option>)}</select></label>
        <label>Mode<select value={argument.mode} onChange={(e) => setArgument({ ...argument, mode: e.target.value })}>{argumentModes.map((x) => <option key={x}>{x}</option>)}</select></label>
        <label>Rationale<textarea value={argument.rationale} maxLength={4096} onChange={(e) => setArgument({ ...argument, rationale: e.target.value })} /></label>
        <button disabled={busy}>Create argument</button>
      </form>

      <form onSubmit={submit(() => {
        const body = groundingRequest(grounding.effect, grounding.adequacy, grounding.rationale, selectedGroundNodes, groundRoles, selectedGroundEvidence, groundMemberRationales);
        if (!grounding.argument) throw new Error('Choose an argument.');
        createGrounding.mutate({ argumentId: grounding.argument, body });
      })}>
        <h3>Add joint grounding set</h3>
        <label>Argument<select value={grounding.argument} onChange={(e) => setGrounding({ ...grounding, argument: e.target.value })}><option value="">Choose…</option>{data.arguments.map((a) => <option key={a.argument_id} value={a.argument_id}>{a.mode}: conclusion {a.conclusion_object_id}</option>)}</select></label>
        <label>Effect<select value={grounding.effect} onChange={(e) => setGrounding({ ...grounding, effect: e.target.value })}>{groundingEffects.map((x) => <option key={x}>{x}</option>)}</select></label>
        <label>Adequacy<select value={grounding.adequacy} onChange={(e) => setGrounding({ ...grounding, adequacy: e.target.value })}>{groundingAdequacies.map((x) => <option key={x}>{x}</option>)}</select></label>
        <fieldset><legend>Joint grounds</legend>
          {data.nodes.map((n) => <div key={n.object_id} className="ground-member-row"><label><input type="checkbox" checked={selectedGroundIds.includes(n.object_id)} onChange={(e) => setSelectedGroundIds((current) => e.target.checked ? [...current, n.object_id] : current.filter((id) => id !== n.object_id))} /> {n.kind}: {n.text}</label>{selectedGroundIds.includes(n.object_id) && <><select value={groundRoles[n.object_id] ?? (n.kind === 'ASSUMPTION' ? 'ASSUMPTION' : 'PREMISE')} onChange={(e) => setGroundRoles({ ...groundRoles, [n.object_id]: e.target.value })}><option>PREMISE</option><option>ASSUMPTION</option><option>CONTEXT</option></select><input aria-label={`Why ${n.kind} is a ground`} placeholder="Why this object belongs in this grounding set" value={groundMemberRationales[`object:${n.object_id}`] ?? ''} maxLength={2048} onChange={(e) => setGroundMemberRationales({ ...groundMemberRationales, [`object:${n.object_id}`]: e.target.value })} /></>}</div>)}
          <p><strong>Documentary evidence</strong></p>
          <div className="evidence-search"><input aria-label="Search documentary evidence" placeholder="Search all attached admissible fragments" value={evidenceSearchText} maxLength={512} onChange={(e) => setEvidenceSearchText(e.target.value)} /><button type="button" disabled={searchEvidence.isPending || !evidenceSearchText.trim()} onClick={() => searchEvidence.mutate(evidenceSearchText.trim())}>Search evidence</button></div>
          {searchEvidence.error && <small className="error">Evidence search failed: {searchEvidence.error.message}</small>}
          {evidenceChoices.map((item) => <div key={item.evidence_id} className="ground-member-row evidence-ground"><label><input type="checkbox" checked={selectedEvidenceIds.includes(item.evidence_id)} onChange={(e) => setSelectedEvidenceIds((current) => e.target.checked ? [...current, item.evidence_id] : current.filter((id) => id !== item.evidence_id))} /> EVIDENCE · {item.document_locator} · #{item.ordinal} · {item.speaker_label}: {item.content}</label>{selectedEvidenceIds.includes(item.evidence_id) && <input aria-label="Why this evidence is a ground" placeholder="Why this exact fragment belongs in this grounding set" value={groundMemberRationales[`evidence:${item.evidence_id}`] ?? ''} maxLength={2048} onChange={(e) => setGroundMemberRationales({ ...groundMemberRationales, [`evidence:${item.evidence_id}`]: e.target.value })} />}</div>)}
          {data.evidence_truncated && <small>Showing the first 256 admissible source fragments attached to this Situation. Search above to reach material outside this bounded list.</small>}
        </fieldset>
        <label>Why these grounds belong together<textarea value={grounding.rationale} maxLength={4096} onChange={(e) => setGrounding({ ...grounding, rationale: e.target.value })} /></label>
        <button disabled={busy}>Create grounding set</button>
      </form>
      <form onSubmit={submit(() => {
        const route = data.grounding_sets.find((g) => `${g.argument_id}:${g.grounding_set_id}` === applicability.route);
        if (!route) throw new Error('Choose an authored route.');
        recordApplicability.mutate({route, body: applicabilityRequest(applicability.condition, applicability.assessment, applicability.rationale)});
      })}>
        <h3>Assess inferential applicability</h3>
        <p>Record a required condition and the investigator’s assessment of it; this is not a truth verdict for the premises or conclusion.</p>
        <label>Grounding route<select value={applicability.route} onChange={(e) => setApplicability({...applicability, route: e.target.value})}><option value="">Choose…</option>{data.grounding_sets.map((g) => <option key={`${g.argument_id}:${g.grounding_set_id}`} value={`${g.argument_id}:${g.grounding_set_id}`}>{g.effect}/{g.adequacy}: {g.rationale}</option>)}</select></label>
        <label>Required condition<select value={applicability.condition} onChange={(e) => setApplicability({...applicability, condition: e.target.value})}><option value="">Choose…</option>{data.nodes.filter((n) => n.kind !== 'INFORMATION_GAP').map((n) => <option key={n.object_id} value={n.object_id}>{n.kind}: {n.text}</option>)}</select></label>
        <label>Condition assessment<select value={applicability.assessment} onChange={(e) => setApplicability({...applicability, assessment: e.target.value})}>{applicabilityAssessments.map((a) => <option key={a}>{a}</option>)}</select></label>
        <label>Assessment rationale<textarea value={applicability.rationale} maxLength={4096} onChange={(e) => setApplicability({...applicability, rationale: e.target.value})} /></label>
        <button disabled={busy}>Record applicability assessment</button>
      </form>
      <form onSubmit={submit(() => {
        const previous = assessmentHistory.find(a => a.event_id === assessment.revises_event_id);
        const target = previous?.input.target ?? assessmentChoices.find(c => assessmentKey(c.target) === assessment.target)?.target;
        recordAssessment.mutate(assessmentRequest(assessment, target, assessment.grounds), {onSuccess: () => setAssessment({target: '', dimension: 'CREDIBILITY', result: 'UNKNOWN', confidence: 'UNKNOWN', criterion: '', context: '', rationale: '', grounds: []})});
      })}>
        <h3>Record contextual epistemic assessment</h3>
        <p>One attributed dimension at a time. Confidence refers to the judgment, not the truth of the target.</p>
        <p>An assessment may itself be assessed. The judgment targets an exact event, not its later revisions or the truth of whatever it assessed.</p>
        {assessment.revises_event_id && <p>Revising {assessment.revises_event_id} <button type="button" onClick={() => setAssessment({...assessment, revises_event_id: null})}>Cancel revision</button></p>}
        <label>Target<select disabled={!!assessment.revises_event_id} value={assessment.target} onChange={e => setAssessment({...assessment, target: e.target.value})}><option value="">Choose…</option>{assessmentChoices.map(c => <option key={assessmentKey(c.target)} value={assessmentKey(c.target)}>{c.label}</option>)}</select></label>
        <label>Dimension<select disabled={!!assessment.revises_event_id} value={assessment.dimension} onChange={e => setAssessment({...assessment, dimension: e.target.value, result: 'UNKNOWN'})}>{Object.keys(assessmentResults).map(x => <option key={x}>{x}</option>)}</select></label>
        <label>Result<select value={assessment.result} onChange={e => setAssessment({...assessment, result: e.target.value})}>{assessmentResults[assessment.dimension].map(x => <option key={x}>{x}</option>)}</select></label>
        <label>Confidence<select value={assessment.confidence} onChange={e => setAssessment({...assessment, confidence: e.target.value})}>{assessmentConfidences.map(x => <option key={x}>{x}</option>)}</select></label>
        <label>Criterion<input disabled={!!assessment.revises_event_id} value={assessment.criterion} maxLength={2048} onChange={e => setAssessment({...assessment, criterion: e.target.value})}/></label>
        <label>Context<input disabled={!!assessment.revises_event_id} value={assessment.context} maxLength={2048} onChange={e => setAssessment({...assessment, context: e.target.value})}/></label>
        <label>Rationale<textarea value={assessment.rationale} maxLength={4096} onChange={e => setAssessment({...assessment, rationale: e.target.value})}/></label>
        <fieldset><legend>Explicit grounds (optional)</legend>{assessmentChoices.filter(c => ['OBJECT','EVIDENCE'].includes(c.target.kind)).map(c => <label key={assessmentKey(c.target)}><input type="checkbox" checked={assessment.grounds.some(g => assessmentKey(g) === assessmentKey(c.target))} onChange={e => setAssessment({...assessment, grounds: e.target.checked ? [...assessment.grounds,c.target] : assessment.grounds.filter(g => assessmentKey(g) !== assessmentKey(c.target))})}/>{c.label}</label>)}</fieldset>
        <button disabled={busy}>Record assessment</button>
      </form>
      <form onSubmit={submit(() => {
        const route = data.grounding_sets.find(g => `${g.argument_id}:${g.grounding_set_id}` === dependency.route);
        recordDependency.mutate(assessmentDependencyRequest(route, dependency.assessment, dependency.rationale));
      })}>
        <h3>Declare reliance on an assessment</h3>
        <p>Pin the exact attributed assessment used by a route. Merely assessing an object does not establish reliance.</p>
        <label>Route<select value={dependency.route} onChange={e => setDependency({...dependency,route:e.target.value})}><option value="">Choose…</option>{data.grounding_sets.map(g => <option key={`${g.argument_id}:${g.grounding_set_id}`} value={`${g.argument_id}:${g.grounding_set_id}`}>{g.rationale}</option>)}</select></label>
        <label>Assessment event<select value={dependency.assessment} onChange={e => setDependency({...dependency,assessment:e.target.value})}><option value="">Choose…</option>{assessmentHistory.map(a => <option key={a.event_id} value={a.event_id}>{a.input.dimension}: {a.input.result} — {a.input.criterion} ({a.event_id})</option>)}</select></label>
        <label>Why this route relies on this judgment<textarea value={dependency.rationale} maxLength={4096} onChange={e => setDependency({...dependency,rationale:e.target.value})}/></label>
        <button disabled={busy}>Record reliance</button>
      </form>
    </div>

    <article><h3>Current structured model</h3>
      {assessmentDependencies.some(v => v.downstream_reviews?.length) && <details open><summary>Downstream assessment review obligations</summary><p>Each reachable route has its own review of the exact assessment revision. Retaining one route does not clear another. No disposition establishes truth.</p><ul>{assessmentDependencies.flatMap(view => (view.downstream_reviews ?? []).map(item => {
        const id = view.dependency.dependency_id;
        const route = item.route;
        const key = `${id}:${view.current_assessment_event_id}:${route.argument_id}:${route.grounding_set_id}`;
        const draft = dependencyReviews[key] ?? {disposition:'DEFERRED',note:''};
        return <li key={key}><strong>{routeLabel(route)}</strong><p>{item.requires_review ? 'Review required' : 'Retained after review'} · assessment revision {view.current_assessment_event_id} · reliance {id}</p>{item.latest_review && <p>{item.latest_review.input.disposition} — {item.latest_review.input.note}</p>}<div className="reassessment-review"><select aria-label="Downstream review disposition" value={draft.disposition} onChange={e => setDependencyReviews({...dependencyReviews,[key]:{...draft,disposition:e.target.value}})}>{reassessmentDispositions.map(x => <option key={x}>{x}</option>)}</select><input aria-label="Downstream review note" maxLength={4096} value={draft.note} onChange={e => setDependencyReviews({...dependencyReviews,[key]:{...draft,note:e.target.value}})}/><button disabled={busy} type="button" onClick={() => {try {reviewDownstream.mutate({id,body:assessmentDownstreamReviewRequest(view,route,draft.disposition,draft.note)});setError('');} catch(e) {setError(e.message);}}}>Record downstream review</button></div>{item.review_history.length > 0 && <details><summary>Downstream review history</summary><ul>{item.review_history.map(r => <li key={r.review_event_id}>{r.input.disposition} — {r.input.note} · {r.recorded_by} · revision {r.input.reviewed_assessment_event_id}</li>)}</ul></details>}</li>;
      }))}</ul></details>}
      {assessmentDependencies.length > 0 && <details open><summary>Assessment reliance and selective review</summary><p>Revision of a pinned assessment creates a review obligation, not a revised truth value. Potential downstream routes are not automatically reviewed by retaining this route.</p><ul>{assessmentDependencies.map(view => {
        const d = view.dependency;
        const draftKey = `${d.dependency_id}:${view.current_assessment_event_id}`;
        const draft = dependencyReviews[draftKey] ?? {disposition:'DEFERRED',note:''};
        return <li key={d.dependency_id}><strong>{routeLabel(d.input)}</strong><p>{d.input.rationale}</p><small>Relies on {d.input.assessment_event_id} · recorded by {d.recorded_by}</small><p>{view.assessment_revised ? `Latest revision: ${view.current_assessment_event_id} — ${view.requires_review ? 'review required' : 'route retained after review'}` : 'No revision of pinned assessment recorded.'}</p>{view.potential_downstream_routes.length > 0 && <details><summary>Potential downstream routes ({view.potential_downstream_routes.length})</summary><ul>{view.potential_downstream_routes.map(r => <li key={`${r.argument_id}:${r.grounding_set_id}`}>{routeLabel(r)}</li>)}</ul></details>}{view.review_history.length > 0 && <details><summary>Review history</summary><ul>{view.review_history.map(r => <li key={r.review_event_id}>{r.input.disposition} — {r.input.note} · {r.recorded_by} · assessment {r.input.reviewed_assessment_event_id}</li>)}</ul></details>}{view.assessment_revised && <div className="reassessment-review"><select aria-label="Assessment reliance disposition" value={draft.disposition} onChange={e => setDependencyReviews({...dependencyReviews,[draftKey]:{...draft,disposition:e.target.value}})}>{reassessmentDispositions.map(x => <option key={x}>{x}</option>)}</select><input aria-label="Assessment reliance review note" value={draft.note} maxLength={4096} onChange={e => setDependencyReviews({...dependencyReviews,[draftKey]:{...draft,note:e.target.value}})}/><button type="button" disabled={busy} onClick={() => {try {reviewDependency.mutate({id:d.dependency_id, body:assessmentDependencyReviewRequest(view,draft.disposition,draft.note)});setError('');} catch(e) {setError(e.message);}}}>Record review</button></div>}</li>;
      })}</ul></details>}
      {assessmentHistory.length > 0 && <details open><summary>Attributed multidimensional assessments — no universal score</summary><p>Separate judgments and disagreements remain visible. A revision preserves its predecessor.</p><ul>{assessmentHistory.map(a => <li key={a.event_id}><strong>{a.input.dimension}: {a.input.result}</strong> · confidence {a.input.confidence} · {supersededAssessments.has(a.event_id) ? 'revised' : 'current in its own revision chain'}<p>{assessmentLabel(a.input.target)}</p><p>Criterion: {a.input.criterion} · Context: {a.input.context}</p><p>{a.input.rationale}</p><small>Recorded by {a.recorded_by} · {a.recorded_at}{a.input.revises_event_id ? ` · revises ${a.input.revises_event_id}` : ''}</small><ul>{a.input.grounds.map(g => <li key={assessmentKey(g)}>{assessmentLabel(g)}</li>)}</ul>{!a.input.grounds.length && <small>No explicit grounds recorded.</small>}{!supersededAssessments.has(a.event_id) && <button type="button" disabled={busy} onClick={() => setAssessment({...a.input, target: assessmentKey(a.input.target), revises_event_id: a.event_id})}>Revise own judgment</button>}</li>)}</ul></details>}
      <p>{data.nodes.length} objects · {data.evidence.length} visible evidence fragments · {data.relations.length} typed relations · {data.arguments.length} arguments · {data.grounding_sets.length} grounding sets</p>
      <ul>{data.nodes.map((n) => <li key={n.object_id}><strong>{n.kind}</strong> — {n.text}<br/><small>{n.rationale}</small></li>)}</ul>
      {data.relations.length > 0 && <details><summary>Relations</summary><ul>{data.relations.map((r) => <li key={r.relation_id}>{r.family}/{r.kind}: {r.left_object_id} → {r.right_object_id}</li>)}</ul></details>}
      {data.grounding_sets.length > 0 && <details><summary>Grounding</summary><ul>{data.grounding_sets.map((g) => <li key={`${g.argument_id}:${g.grounding_set_id}`}>{g.effect}/{g.adequacy}: {g.members.length} joint member(s)</li>)}</ul></details>}
      {data.reassessment_impacts.length > 0 && <details open><summary>Selective reassessment impacts</summary><ul>{data.reassessment_impacts.map((impact) => {
        const key = `${impact.trigger_relation_id}:${impact.argument_id}:${impact.grounding_set_id ?? 'conclusion'}`;
        const draft = reassessmentDrafts[key] ?? { disposition: impact.latest_review?.disposition ?? 'DEFERRED', note: '' };
        return <li key={key}><strong>{impact.relation_kind}</strong> changed object {impact.prior_object_id}; review argument {impact.argument_id}{impact.grounding_set_id ? ` / grounding set ${impact.grounding_set_id}` : ' / conclusion'}{impact.depth > 1 ? ` · downstream depth ${impact.depth}` : ''}.
          {impact.latest_review && <small> Latest review: {impact.latest_review.disposition} — {impact.latest_review.note}</small>}
          <div className="reassessment-review"><select aria-label="Reassessment disposition" value={draft.disposition} onChange={(e) => setReassessmentDrafts({ ...reassessmentDrafts, [key]: { ...draft, disposition: e.target.value } })}>{reassessmentDispositions.map((x) => <option key={x}>{x}</option>)}</select><input aria-label="Reassessment review note" placeholder="Why retain, revise, or defer this argument" value={draft.note} maxLength={4096} onChange={(e) => setReassessmentDrafts({ ...reassessmentDrafts, [key]: { ...draft, note: e.target.value } })} /><button type="button" disabled={busy} onClick={() => { try { reviewReassessment.mutate({ impact, body: reassessmentReviewRequest(impact.grounding_set_id, draft.disposition, draft.note) }); setError(''); } catch (e) { setError(e.message); } }}>Record review</button></div>
        </li>;
      })}</ul></details>}
      {data.grounding_cycles.length > 0 && <details open><summary>Grounding cycle diagnostics</summary><ul>{data.grounding_cycles.map((cycle) => <li key={cycle.object_ids.join(':')}><strong>Recursive support:</strong> {cycle.object_ids.length} object(s), {cycle.argument_ids.length} argument(s). {cycle.has_external_anchor ? 'External grounds are recorded; the cycle itself adds no independent corroboration.' : 'No external supporting ground is recorded; recursive support must not be treated as corroboration.'} {cycle.external_only_grounding_routes?.length ?? 0} external-only route(s); external grounds do not establish independence, reliability or sufficiency. Context alone is not support.</li>)}</ul></details>}
      {data.reasoning_diagnostics.length > 0 && <details open><summary>Reasoning diagnostics — structural, not a verdict</summary><ul>{data.reasoning_diagnostics.map((diagnostic) => {
        const conclusion = data.nodes.find((node) => node.object_id === diagnostic.conclusion_object_id);
        const directional = diagnostic.routes.filter((route) => route.non_context_member_count > 0);
        const supports = directional.filter((route) => route.effect === 'SUPPORTS').length;
        const opposes = directional.filter((route) => route.effect === 'WEAKENS' || route.effect === 'CHALLENGES').length;
        const conditions = directional.filter((route) => route.effect === 'CONDITIONS').length;
        const groundText = (ground) => ground.ground_object_id ? (data.nodes.find((node) => node.object_id === ground.ground_object_id)?.text ?? ground.ground_object_id) : (data.evidence.find((item) => item.evidence_id === ground.evidence_id)?.content ?? ground.evidence_id);
        return <li key={diagnostic.conclusion_object_id}><strong>{conclusion?.kind ?? 'CONCLUSION'}:</strong> {conclusion?.text ?? diagnostic.conclusion_object_id}<br/><small>{diagnostic.routes.length} authored route(s): {supports} support · {opposes} weaken/challenge · {conditions} condition · {diagnostic.routes.length - directional.length} context-only. Niccolò does not net opposing routes into a truth or confidence score.</small>
          {diagnostic.has_mixed_directional_grounding && <p className="epistemic-boundary">Support and opposition coexist; neither cancels the other.</p>}
          {diagnostic.conclusion_reassessment.impacted > 0 && <p><small>Conclusion-level reassessment: {diagnostic.conclusion_reassessment.unreviewed} unreviewed · {diagnostic.conclusion_reassessment.deferred} deferred · {diagnostic.conclusion_reassessment.revision_required} revision required · {diagnostic.conclusion_reassessment.retained} retained.</small></p>}
          {diagnostic.shared_grounds?.length > 0 && <details><summary>Shared grounds — route reuse is not independent corroboration</summary><ul>{diagnostic.shared_grounds.map((shared) => <li key={shared.ground_object_id ?? shared.evidence_id}>{groundText(shared)} · {shared.routes.length} routes</li>)}</ul><small>Exact identity reuse only; different grounds may still share provenance.</small></details>}
          {diagnostic.direct_evidence_lineage && diagnostic.direct_evidence_lineage.status !== 'NO_DIRECT_EVIDENCE' && <details open><summary>Recorded documentary dependence</summary>
            {diagnostic.direct_evidence_lineage.status === 'RECORDED_LINEAGE_RESOLVED' ? <><p>{diagnostic.direct_evidence_lineage.selected_evidence_count} directly selected fragment(s) in {diagnostic.direct_evidence_lineage.components.length} recorded dependence component(s).</p><ul>{diagnostic.direct_evidence_lineage.components.map((component) => <li key={component.evidence_ids.join(':')}>{component.evidence_ids.length} fragment(s), {component.routes.length} authored route(s)<ul>{component.evidence_ids.map((id) => <li key={id}>{groundText({evidence_id: id})}</li>)}</ul></li>)}</ul></> : <p>Recorded lineage could not be resolved ({diagnostic.direct_evidence_lineage.status}); no independence inference is justified.</p>}
            <small>Shared recorded origins or source documents indicate possible dependence. Different components do not establish independent corroboration. This diagnostic covers direct documentary grounds only, not indirect object-ground ancestry.</small></details>}
          {diagnostic.routes.length === 0 && <p>No authored grounding route is recorded for this conclusion.</p>}
          <ul>{diagnostic.routes.map((route) => {
            const set = data.grounding_sets.find((g) => g.argument_id === route.argument_id && g.grounding_set_id === route.grounding_set_id);
            const authoredArgument = data.arguments.find((a) => a.argument_id === route.argument_id);
            return <li key={`${route.argument_id}:${route.grounding_set_id}`}>{route.effect}/{route.adequacy} · {route.member_count} joint member(s){route.non_context_member_count === 0 ? ' · context-only' : ''}{route.reassessment.impacted > 0 ? ` · reassessment: ${route.reassessment.unreviewed} unreviewed, ${route.reassessment.deferred} deferred, ${route.reassessment.revision_required} revision required, ${route.reassessment.retained} retained` : ''}{route.unanchored_recursive_support ? ' · WARNING: unanchored recursive support' : route.recursive_support_cycle ? ' · recursive support with external grounds' : ''}
            {route.assessment_reassessment && Object.entries(route.assessment_reassessment).filter(([,attention]) => attention.impacted > 0).map(([kind,attention]) => <p key={kind}><small>{kind === 'directly_relying' ? 'Pinned assessment revisions' : 'Downstream assessment revisions'}: {attention.unreviewed} unreviewed · {attention.deferred} deferred · {attention.revision_required} revision required · {attention.retained} retained. Review counts are not confidence scores.</small></p>)}
            {route.applicability_conditions?.length > 0 ? <details open><summary>Required conditions — attributed assessments</summary><ul>{route.applicability_conditions.map((condition) => <li key={`${condition.condition_object_id}:${condition.recorded_by}`}><strong>{condition.assessment}</strong> · {groundText({ground_object_id: condition.condition_object_id})}<br/><small>{condition.rationale} · recorded by {condition.recorded_by}</small></li>)}</ul><small>Even satisfied conditions do not establish truth or validity. Divergent assessments remain visible; no consensus is inferred. Unsatisfied, contested or unknown conditions require attention before relying on this route.</small></details> : <small> · No applicability condition recorded; applicability is not certified.</small>}
            {set && <details><summary>Inspect authored grounds and rationales</summary>{authoredArgument && <p>{authoredArgument.mode}: {authoredArgument.rationale}</p>}<p>{set.rationale}</p><ul>{set.members.map((member) => <li key={member.member_id}><strong>{member.role}:</strong> {groundText(member)}<br/><small>{member.rationale}</small></li>)}</ul></details>}</li>;
          })}</ul>
        </li>;
      })}</ul></details>}
    </article>
  </section>;
}
