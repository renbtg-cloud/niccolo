import { useMemo, useRef, useState } from 'react';
import { useMutation, useQuery } from '@tanstack/react-query';
import { useClient } from '../hooks/useClient.js';
import { ResponseInterpretationPanel } from './ResponseInterpretationPanel.jsx';
import { InterpretationRelianceReview } from './InterpretationRelianceReview.jsx';
import { InquiryComparisonPanel } from './InquiryComparisonPanel.jsx';
import { ActorEpistemicStatePanel } from './ActorEpistemicStatePanel.jsx';
import { ActorEpistemicActPanel } from './ActorEpistemicActPanel.jsx';
import {
  argumentModes,
  argumentRequest,
  applicabilityAssessments,
  applicabilityRequest,
  assessmentResults, assessmentConfidences, assessmentKey, assessmentRequest, assessmentTargetRevision, assessmentDimensionsForTarget, selectionCriteria,
  assessmentDependencyRequest, assessmentDependencyReviewRequest, assessmentDownstreamReviewRequest,
  groundingAdequacies,
  groundingEffects,
  groundingRequest,
  defaultGroundRole, relationEndpointAllowed,
  nodeKinds,
  nodeRequest,
  questionResponseRequest,
  interpretationDependencyRequest,
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
  const actors = useQuery({queryKey:['epistemic-response-actors',client?.config.workspace],enabled:!!client,queryFn:async () => (await client.actors()).value.actors});
  const [responseDraft, setResponseDraft] = useState({question:'',text:'',speaker:'',evidence:'',rationale:''});
  const pendingResponse = useRef(null);
  const [error, setError] = useState('');
  const [assessment, setAssessment] = useState({ target: '', dimension: 'CREDIBILITY', result: 'UNKNOWN', confidence: 'UNKNOWN', criterion: '', context: '', rationale: '', grounds: [] });
  const [dependency, setDependency] = useState({route: '', assessment: '', rationale: ''});
  const [meaningDependency, setMeaningDependency] = useState({route:'',revision:'',rationale:''});
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
  const recordResponse = useWorkbenchMutation(client, workbench, async ({questionId,responseId,body}) => client.recordExpertQuestionResponse(questionId,responseId,body));
  const recordDependency = useWorkbenchMutation(client, workbench, async (body) => client.recordExpertAssessmentDependency(crypto.randomUUID(), body));
  const recordMeaningDependency = useWorkbenchMutation(client, workbench, async (body) => client.recordInterpretationDependency(crypto.randomUUID(), body));
  const reviewDependency = useWorkbenchMutation(client, workbench, async ({id, body}) => client.reviewExpertAssessmentDependency(id, crypto.randomUUID(), body));
  const reviewDownstream = useWorkbenchMutation(client, workbench, async ({id, body}) => client.reviewExpertAssessmentDownstream(id, crypto.randomUUID(), body));
  const mutations = [createNode, createRelation, createArgument, createGrounding, reviewReassessment, recordApplicability, recordAssessment, recordResponse, recordDependency, recordMeaningDependency, reviewDependency, reviewDownstream];
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
  const selectedAssessmentTarget = assessmentChoices.find(c => assessmentKey(c.target) === assessment.target)?.target;
  const assessmentDimensions = assessmentDimensionsForTarget(selectedAssessmentTarget, data.nodes);
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
      {data.nodes.some(n => n.kind === 'QUESTION') && <form onSubmit={submit(() => {
        const key = JSON.stringify([client.config.workspace, client.config.situation, responseDraft]);
        if (pendingResponse.current?.key !== key) pendingResponse.current = {key, responseId:crypto.randomUUID(), utteranceId:crypto.randomUUID()};
        const pending = pendingResponse.current;
        recordResponse.mutate({questionId:responseDraft.question,responseId:pending.responseId,body:questionResponseRequest(responseDraft,pending.utteranceId)}, {onSuccess:() => {pendingResponse.current=null;setResponseDraft({question:'',text:'',speaker:'',evidence:'',rationale:''});}});
      })}>
        <h3>Record a response already received</h3><p>Preserve the original English utterance as a communicative occurrence. Recording does not assert its content, validate its meaning, close a gap or send a new question.</p>
        <label>Question<select value={responseDraft.question} onChange={e => setResponseDraft({...responseDraft,question:e.target.value})}><option value="">Choose…</option>{data.nodes.filter(n => n.kind === 'QUESTION').map(n => <option key={n.object_id} value={n.object_id}>{n.text}</option>)}</select></label>
        <label>Original utterance<textarea value={responseDraft.text} maxLength={8192} onChange={e => setResponseDraft({...responseDraft,text:e.target.value})}/></label>
        <label>Speaker, if identified<select value={responseDraft.speaker} onChange={e => setResponseDraft({...responseDraft,speaker:e.target.value})}><option value="">Unspecified speaker</option>{(actors.data ?? []).map(a => <option key={a.actor_id} value={a.actor_id}>{a.display_label}</option>)}</select></label>{actors.error && <small>Actor registry unavailable; no speaker is inferred.</small>}
        <label>Source fragment, if recorded<select value={responseDraft.evidence} onChange={e => setResponseDraft({...responseDraft,evidence:e.target.value})}><option value="">No explicit source fragment</option>{evidenceChoices.map(e => <option key={e.evidence_id} value={e.evidence_id}>{e.document_locator} #{e.ordinal}: {e.content}</option>)}</select></label>
        <label>Why this utterance responds to the question<textarea value={responseDraft.rationale} maxLength={4096} onChange={e => setResponseDraft({...responseDraft,rationale:e.target.value})}/></label>
        <button disabled={busy}>Record response occurrence</button>
      </form>}
      <form onSubmit={submit(() => createNode.mutate(nodeRequest(node.kind, node.text, node.rationale)))}>
        <h3>Add abstraction</h3>
        <label>Kind<select value={node.kind} onChange={(e) => setNode({ ...node, kind: e.target.value })}>{nodeKinds.map((x) => <option key={x}>{x}</option>)}</select></label>
        <label>{node.kind === 'QUESTION' ? 'Investigative question (not an assertion)' : 'Structured statement'}<textarea value={node.text} maxLength={8192} onChange={(e) => setNode({ ...node, text: e.target.value })} /></label>
        <label>Why this abstraction is appropriate<textarea value={node.rationale} maxLength={4096} onChange={(e) => setNode({ ...node, rationale: e.target.value })} /></label>
        <button disabled={busy}>Create object</button>
      </form>

      <form onSubmit={submit(() => createRelation.mutate(relationRequest(relation.family, relation.kind, relation.left, relation.right, relation.rationale)))}>
        <h3>Add typed relation</h3>
        <label>Family<select value={relation.family} onChange={(e) => setRelation({ ...relation, family: e.target.value, kind: relationKinds[e.target.value][0], left:'', right:'' })}>{Object.keys(relationKinds).map((x) => <option key={x}>{x}</option>)}</select></label>
        <label>Relation<select value={relation.kind} onChange={(e) => setRelation({ ...relation, kind: e.target.value, left:'', right:'' })}>{relationKinds[relation.family].map((x) => <option key={x}>{x}</option>)}</select></label>
        <small className="relation-semantics">{relationSemantics[relation.kind].description}</small>
        <label>{relationSemantics[relation.kind].left}<select value={relation.left} onChange={(e) => setRelation({ ...relation, left: e.target.value })}><option value="">Choose…</option>{data.nodes.filter(n => relationEndpointAllowed(relation.family, relation.kind, n.kind, data.nodes.find(o => o.object_id === relation.right)?.kind)).map((n) => <option key={n.object_id} value={n.object_id}>{n.kind}: {n.text}</option>)}</select></label>
        <label>{relationSemantics[relation.kind].right}<select value={relation.right} onChange={(e) => setRelation({ ...relation, right: e.target.value })}><option value="">Choose…</option>{data.nodes.filter(n => relationEndpointAllowed(relation.family, relation.kind, n.kind, data.nodes.find(o => o.object_id === relation.left)?.kind, 'right')).map((n) => <option key={n.object_id} value={n.object_id}>{n.kind}: {n.text}</option>)}</select></label>
        <label>Rationale<textarea value={relation.rationale} maxLength={4096} onChange={(e) => setRelation({ ...relation, rationale: e.target.value })} /></label>
        <button disabled={busy}>Create relation</button>
      </form>

      <form onSubmit={submit(() => createArgument.mutate(argumentRequest(argument.conclusion, argument.mode, argument.rationale)))}>
        <h3>Add argument</h3>
        <label>Conclusion<select value={argument.conclusion} onChange={(e) => setArgument({ ...argument, conclusion: e.target.value })}><option value="">Choose…</option>{data.nodes.filter(n => n.kind !== 'QUESTION').map((n) => <option key={n.object_id} value={n.object_id}>{n.kind}: {n.text}</option>)}</select></label>
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
          {data.nodes.map((n) => <div key={n.object_id} className="ground-member-row"><label><input type="checkbox" checked={selectedGroundIds.includes(n.object_id)} onChange={(e) => setSelectedGroundIds((current) => e.target.checked ? [...current, n.object_id] : current.filter((id) => id !== n.object_id))} /> {n.kind}: {n.text}</label>{selectedGroundIds.includes(n.object_id) && <><select value={groundRoles[n.object_id] ?? defaultGroundRole(n.kind)} onChange={(e) => setGroundRoles({ ...groundRoles, [n.object_id]: e.target.value })}>{(n.kind === 'QUESTION' ? ['CONTEXT'] : ['PREMISE','ASSUMPTION','CONTEXT']).map(role => <option key={role}>{role}</option>)}</select><input aria-label={`Why ${n.kind} is a ground`} placeholder="Why this object belongs in this grounding set" value={groundMemberRationales[`object:${n.object_id}`] ?? ''} maxLength={2048} onChange={(e) => setGroundMemberRationales({ ...groundMemberRationales, [`object:${n.object_id}`]: e.target.value })} /></>}</div>)}
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
        <label>Required condition<select value={applicability.condition} onChange={(e) => setApplicability({...applicability, condition: e.target.value})}><option value="">Choose…</option>{data.nodes.filter((n) => !['INFORMATION_GAP','QUESTION'].includes(n.kind)).map((n) => <option key={n.object_id} value={n.object_id}>{n.kind}: {n.text}</option>)}</select></label>
        <label>Condition assessment<select value={applicability.assessment} onChange={(e) => setApplicability({...applicability, assessment: e.target.value})}>{applicabilityAssessments.map((a) => <option key={a}>{a}</option>)}</select></label>
        <label>Assessment rationale<textarea value={applicability.rationale} maxLength={4096} onChange={(e) => setApplicability({...applicability, rationale: e.target.value})} /></label>
        <button disabled={busy}>Record applicability assessment</button>
      </form>
      <form onSubmit={submit(() => {
        const previous = assessmentHistory.find(a => a.event_id === assessment.revises_event_id);
        const target = previous?.input.target ?? assessmentChoices.find(c => assessmentKey(c.target) === assessment.target)?.target;
        recordAssessment.mutate(assessmentRequest(assessment, target, assessment.grounds, [...data.nodes, ...data.arguments.map(a => ({object_id:a.argument_id, kind:'ARGUMENT'}))]), {onSuccess: () => setAssessment({target: '', dimension: 'CREDIBILITY', result: 'UNKNOWN', confidence: 'UNKNOWN', criterion: '', context: '', rationale: '', grounds: []})});
      })}>
        <h3>Record contextual epistemic assessment</h3>
        <p>One attributed dimension at a time. Confidence refers to the judgment, not the truth of the target.</p>
        <p>An assessment may itself be assessed. The judgment targets an exact event, not its later revisions or the truth of whatever it assessed.</p>
        {assessment.revises_event_id && <p>Revising {assessment.revises_event_id} <button type="button" onClick={() => setAssessment({...assessment, revises_event_id: null})}>Cancel revision</button></p>}
        <label>Target<select disabled={!!assessment.revises_event_id} value={assessment.target} onChange={e => {
          const target = assessmentChoices.find(c => assessmentKey(c.target) === e.target.value)?.target;
          const compatible = assessmentDimensionsForTarget(target, data.nodes).includes(assessment.dimension);
          setAssessment({...assessment, target: e.target.value, ...(compatible ? {} : {dimension:'CREDIBILITY',result:'UNKNOWN'})});
        }}><option value="">Choose…</option>{assessmentChoices.map(c => <option key={assessmentKey(c.target)} value={assessmentKey(c.target)}>{c.label}</option>)}</select></label>
        <label>Dimension<select disabled={!!assessment.revises_event_id} value={assessment.dimension} onChange={e => setAssessment({...assessment, dimension: e.target.value, result: 'UNKNOWN'})}>{assessmentDimensions.map(x => <option key={x}>{x}</option>)}</select></label>
        {assessment.dimension === 'GAP_RESOLUTION' && <p>Assess the missing aspect independently of questions or answers. PARTIALLY_ADDRESSED and SUPPORTED require an explicit ground other than a question or information gap. The assessment is not a truth verdict.</p>}
        {selectionCriteria.includes(assessment.dimension) && <p>Record a contextual judgment about this question. Reported authorization is not a permission grant; no judgment authorizes execution, establishes truth, or reallocates resources.</p>}
        {assessment.dimension.startsWith('INTEGRITY_') && <p>Assess only the selected aspect in context. No concern indicated does not prove integrity; a concern or indicated compromise is contestable and does not automatically invalidate the investigation or veto a decision.</p>}
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
      {(data.response_interpretations ?? []).length > 0 && <form onSubmit={submit(() => {
        const route=data.grounding_sets.find(g=>`${g.argument_id}:${g.grounding_set_id}`===meaningDependency.route);
        const [id,revision]=meaningDependency.revision.split(':');
        const view=data.response_interpretations.find(v=>v.interpretation_id===id);
        recordMeaningDependency.mutate(interpretationDependencyRequest(route,view,revision,meaningDependency.rationale));
      })}><h3>Declare reliance on interpreted meaning</h3><p>Pin the exact revision and its recorded confirmation context. Unconfirmed meaning may be used provisionally; confirmed meaning is not verified truth.</p>
        <label>Route<select value={meaningDependency.route} onChange={e=>setMeaningDependency({...meaningDependency,route:e.target.value})}><option value="">Choose…</option>{data.grounding_sets.map(g=><option key={`${g.argument_id}:${g.grounding_set_id}`} value={`${g.argument_id}:${g.grounding_set_id}`}>{g.rationale}</option>)}</select></label>
        <label>Interpretation revision<select value={meaningDependency.revision} onChange={e=>setMeaningDependency({...meaningDependency,revision:e.target.value})}><option value="">Choose…</option>{data.response_interpretations.flatMap(v=>v.revisions.map(r=><option key={`${v.interpretation_id}:${r.revision_id}`} value={`${v.interpretation_id}:${r.revision_id}`}>{r.meaning_confirmation} · {r.revision_id===v.current_revision_id?'current':'historical'} · {r.interpretation_text}</option>))}</select></label>
        <label>Why this route relies on this meaning<textarea value={meaningDependency.rationale} maxLength={4096} onChange={e=>setMeaningDependency({...meaningDependency,rationale:e.target.value})}/></label><button disabled={busy}>Record interpretation reliance</button>
      </form>}
    </div>

    <article><h3>Current structured model</h3>
      {(data.interpretation_dependencies ?? []).length>0 && <details open><summary>Interpretation reliance — selective reassessment</summary><p>Meaning revision or changed confirmation context prompts attention only for explicitly relying routes and their potential downstream routes. It never rewrites a conclusion. Retaining the directly relying route does not review downstream routes.</p><ul>{data.interpretation_dependencies.map(view=><li key={view.dependency.dependency_id}><strong>{routeLabel(view.dependency.input)}</strong><p>{view.dependency.input.rationale}</p><small>Pinned revision {view.dependency.input.revision_id} · confirmation {view.dependency.input.confirmation_event_id ?? 'none recorded'} · recorded by {view.dependency.recorded_by}</small><p>{view.interpretation_revised?(view.requires_review?'Interpretation revised; reassessment required.':'Interpretation revised; directly relying route retained.'):view.meaning_confirmation_changed?(view.requires_review?'Meaning confirmation changed; reassessment required.':'Meaning confirmation changed; directly relying route retained.'):'No change to pinned meaning or confirmation context.'}</p>{(view.interpretation_revised||view.meaning_confirmation_changed)&&<><p>Current revision {view.current_revision_id} · confirmation {view.current_confirmation_event_id ?? 'none recorded'}</p><InterpretationRelianceReview key={`${view.dependency.dependency_id}:${view.current_revision_id}:${view.current_confirmation_event_id}`} view={view} route={view.dependency.input} reviewView={view} client={client} onChanged={()=>workbench.refetch()}/></>}{view.downstream_reviews?.length>0&&<details open><summary>Potential downstream routes ({view.downstream_reviews.length})</summary><ul>{view.downstream_reviews.map(item=><li key={`${item.route.argument_id}:${item.route.grounding_set_id}:${view.current_revision_id}:${view.current_confirmation_event_id}`}><strong>{routeLabel(item.route)}</strong><InterpretationRelianceReview view={view} route={item.route} reviewView={item} client={client} onChanged={()=>workbench.refetch()}/></li>)}</ul></details>}</li>)}</ul></details>}
      {(data.question_responses ?? []).length > 0 && <details open><summary>Utterance understanding — revisable meaning, not truth</summary>{[...new Map(data.question_responses.map(r => [r.utterance.utterance_id,r.utterance])).values()].map(utterance => <ResponseInterpretationPanel key={`${client.config.workspace}:${client.config.situation}:${utterance.utterance_id}`} utterance={utterance} view={(data.response_interpretations ?? []).find(v => v.utterance_id === utterance.utterance_id)} client={client} onChanged={() => workbench.refetch()}/>)}</details>}
      {data.nodes.some(n => n.kind === 'INFORMATION_GAP') && <details open><summary>Information gaps — attributed resolution judgments</summary><p>A response or a question link does not resolve an information gap. Missing judgments are not findings.</p><ul>{data.nodes.filter(n => n.kind === 'INFORMATION_GAP').map(gap => {
        const judgments = assessmentHistory.filter(a => a.input.dimension === 'GAP_RESOLUTION' && a.input.target.kind === 'OBJECT' && a.input.target.object_id === gap.object_id && !supersededAssessments.has(a.event_id));
        return <li key={gap.object_id}><strong>{gap.text}</strong><p>{gap.rationale}</p>{judgments.length === 0 ? <p>No explicit gap resolution judgment recorded.</p> : <ul>{judgments.map(a => <li key={a.event_id}><strong>{a.input.result}</strong> · confidence {a.input.confidence}<p>{a.input.criterion} · {a.input.context}</p><p>{a.input.rationale}</p><small>Recorded by {a.recorded_by} · {a.event_id}</small><ul>{a.input.grounds.map(g => <li key={assessmentKey(g)}>{assessmentLabel(g)}</li>)}</ul>{!a.input.grounds.length && <small>No explicit grounds recorded.</small>}</li>)}</ul>}</li>;
      })}</ul><small>Current judgments in their own revision chains remain separate, including disagreement. Earlier revisions are retained in assessment history.</small></details>}
      <InquiryComparisonPanel key={`${client.config.workspace}:${client.config.situation}`} snapshot={data} client={client} onChanged={()=>workbench.refetch()}/>
      <ActorEpistemicStatePanel key={`actor-states:${client.config.workspace}:${client.config.situation}`} snapshot={data} actors={actors.data??[]} client={client} onChanged={()=>workbench.refetch()}/>
      <ActorEpistemicActPanel key={`actor-acts:${client.config.workspace}:${client.config.situation}`} snapshot={data} actors={actors.data??[]} client={client} onChanged={()=>workbench.refetch()}/>
      {(data.question_inquiries ?? []).length > 0 && <details open><summary>Investigative questions — explicit inquiry map</summary><p>Links record what to investigate, not answers or gap resolution. Testing multiple hypotheses does not establish discriminatory power.</p><ul>{data.question_inquiries.map(inquiry => {
        const question = data.nodes.find(n => n.object_id === inquiry.question_object_id);
        const linkGroups = [['Addressed gaps', inquiry.addressed_gaps], ['Tested hypotheses', inquiry.tested_hypotheses], ['Explicit presuppositions', inquiry.presuppositions]];
        const criterionLabels = {ACQUISITION_COST:'Acquisition cost',ACCESSIBILITY:'Accessibility',CONTAMINATION_RISK:'Contamination risk',URGENCY:'Urgency',AUTHORIZATION_STATUS:'Reported authorization status (not a permission grant)',INVESTIGATION_COVERAGE:'Investigation coverage'};
        const judgmentGroups = [['Relevance', inquiry.current_relevance_events], ['Discriminatory power', inquiry.current_discriminatory_power_events], ...(inquiry.selection_criteria ?? []).map(c => [criterionLabels[c.dimension] ?? c.dimension,c.current_events])];
        const responses = (data.question_responses ?? []).filter(r => r.question_id === inquiry.question_object_id);
        return <li key={inquiry.question_object_id}><strong>{question?.text ?? inquiry.question_object_id}</strong><p>{question?.rationale}</p><small>Authored by {question?.created_by}</small><div><strong>Recorded response occurrences — not accepted facts</strong>{responses.length === 0 ? <p>No response recorded.</p> : <ul>{responses.map(r => <li key={r.response_id}><blockquote>{r.utterance.utterance_text}</blockquote><small>Speaker: {actors.data?.find(a => a.actor_id === r.utterance.speaker_actor_id)?.display_label ?? r.utterance.speaker_actor_id ?? 'Unspecified'} · source: {r.utterance.source_evidence_id ?? r.utterance.source_contribution_id ?? 'not explicitly linked'}</small><p>{r.rationale}</p><small>Recorded by {r.recorded_by} · {r.recorded_at} · utterance {r.utterance.utterance_id}</small></li>)}</ul>}</div>{linkGroups.map(([label, links]) => <div key={label}><strong>{label}</strong>{links.length === 0 ? <p>No explicit link recorded.</p> : <ul>{links.map(link => {
          const target = data.nodes.find(n => n.object_id === link.target_object_id);
          const relation = data.relations.find(r => r.relation_id === link.relation_id);
          return <li key={link.relation_id}>{target?.kind}: {target?.text ?? link.target_object_id}<br/><small>{relation?.rationale} · recorded by {relation?.recorded_by}</small></li>;
        })}</ul>}</div>)}{judgmentGroups.map(([label, ids]) => <div key={label}><strong>{label} — current judgments in their own revision chains</strong>{ids.length === 0 ? <p>No explicit assessment recorded.</p> : <ul>{ids.map(id => {
          const a = assessmentHistory.find(event => event.event_id === id);
          return <li key={id}>{a ? <>{a.input.result} · confidence {a.input.confidence}<p>{a.input.criterion} · {a.input.context}</p><p>{a.input.rationale}</p><small>Recorded by {a.recorded_by} · {id}</small></> : id}</li>;
        })}</ul>}</div>)}</li>;
      })}</ul><small>Unlinked questions remain visible; missing links or judgments are not negative findings. Divergent judgments are not averaged or treated as consensus.</small></details>}
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
          {diagnostic.contradictory_supports?.length > 0 && <details open><summary>Review candidate — contradictory grounds support the same conclusion</summary><p>Only explicitly authored contradictions and non-contextual supporting grounds are shown. This pattern may warrant examining self-sealing reasoning, but is not proof: conditional or alternative routes can be legitimate. Absence of a warning does not establish falsifiability. No conclusion or route is automatically rejected.</p><ul>{diagnostic.contradictory_supports.map(candidate=><li key={candidate.relation_id}><strong>{groundText({ground_object_id:candidate.left_object_id})}</strong><ul>{candidate.left_supporting_routes.map(route=><li key={`${route.argument_id}:${route.grounding_set_id}`}>{routeLabel(route)} · {route.grounding_set_id}</li>)}</ul><strong>{groundText({ground_object_id:candidate.right_object_id})}</strong><ul>{candidate.right_supporting_routes.map(route=><li key={`${route.argument_id}:${route.grounding_set_id}`}>{routeLabel(route)} · {route.grounding_set_id}</li>)}</ul><small>Authored contradiction {candidate.relation_id} — {data.relations.find(r=>r.relation_id===candidate.relation_id)?.rationale}</small></li>)}</ul></details>}
          {diagnostic.information_gap_supports?.length > 0 && <details open><summary>Review candidate — information gap used as a supporting ground</summary><p>An information gap describes what remains to be investigated; it is not by itself evidence of absence, concealment or misconduct. The routes are authored judgments, not verified negative observations. A gap may legitimately support a need for further inquiry.</p><ul>{diagnostic.information_gap_supports.map(gap=><li key={gap.gap_object_id}><strong>{groundText({ground_object_id:gap.gap_object_id})}</strong><ul>{gap.supporting_routes.map(route=><li key={`${route.argument_id}:${route.grounding_set_id}`}>{routeLabel(route)} · {route.grounding_set_id}</li>)}</ul></li>)}</ul>{diagnostic.contradictory_supports?.length > 0 && <p>Both contradictory grounds and a gap are used as support. Examine whether the conclusion remains open to contrary outcomes; no self-sealing verdict is inferred.</p>}</details>}
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
            {route.interpretation_reassessment && Object.entries(route.interpretation_reassessment).filter(([,attention])=>attention.impacted>0).map(([kind,attention])=><p key={kind}><small>{kind==='directly_relying'?'Pinned interpretation attention':'Downstream interpretation attention'}: {attention.unreviewed} unreviewed · {attention.deferred} deferred · {attention.revision_required} revision required · {attention.retained} retained. Meaning change does not automatically alter truth.</small></p>)}
            {route.applicability_conditions?.length > 0 ? <details open><summary>Required conditions — attributed assessments</summary><ul>{route.applicability_conditions.map((condition) => <li key={`${condition.condition_object_id}:${condition.recorded_by}`}><strong>{condition.assessment}</strong> · {groundText({ground_object_id: condition.condition_object_id})}<br/><small>{condition.rationale} · recorded by {condition.recorded_by}</small></li>)}</ul><small>Even satisfied conditions do not establish truth or validity. Divergent assessments remain visible; no consensus is inferred. Unsatisfied, contested or unknown conditions require attention before relying on this route.</small></details> : <small> · No applicability condition recorded; applicability is not certified.</small>}
            {set && <details><summary>Inspect authored grounds and rationales</summary>{authoredArgument && <p>{authoredArgument.mode}: {authoredArgument.rationale}</p>}<p>{set.rationale}</p><ul>{set.members.map((member) => <li key={member.member_id}><strong>{member.role}:</strong> {groundText(member)}<br/><small>{member.rationale}</small></li>)}</ul></details>}</li>;
          })}</ul>
        </li>;
      })}</ul></details>}
    </article>
  </section>;
}
