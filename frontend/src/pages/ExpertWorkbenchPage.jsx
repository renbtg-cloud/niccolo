import { useMemo, useState } from 'react';
import { useMutation, useQuery } from '@tanstack/react-query';
import { useClient } from '../hooks/useClient.js';
import {
  argumentModes,
  argumentRequest,
  groundingAdequacies,
  groundingEffects,
  groundingRequest,
  nodeKinds,
  nodeRequest,
  relationKinds,
  relationRequest,
  relationSemantics,
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
  const data = workbench.data ?? { nodes: [], evidence: [], evidence_truncated: false, relations: [], arguments: [], grounding_sets: [] };
  const [error, setError] = useState('');
  const [node, setNode] = useState({ kind: 'HYPOTHESIS', text: '', rationale: '' });
  const [relation, setRelation] = useState({ family: 'SEMANTIC', kind: 'CONTRADICTS', left: '', right: '', rationale: '' });
  const [argument, setArgument] = useState({ conclusion: '', mode: 'ABDUCTIVE', rationale: '' });
  const [grounding, setGrounding] = useState({ argument: '', effect: 'SUPPORTS', adequacy: 'PARTIAL', rationale: '' });
  const [selectedGroundIds, setSelectedGroundIds] = useState([]);
  const [selectedEvidenceIds, setSelectedEvidenceIds] = useState([]);
  const [evidenceSearchText, setEvidenceSearchText] = useState('');
  const [groundRoles, setGroundRoles] = useState({});
  const [groundMemberRationales, setGroundMemberRationales] = useState({});

  const searchEvidence = useMutation({ mutationFn: async (query) => (await client.searchExpertEvidence(query, 20)).value });
  const createNode = useWorkbenchMutation(client, workbench, async (body) => client.createExpertNode(crypto.randomUUID(), body));
  const createRelation = useWorkbenchMutation(client, workbench, async (body) => client.createExpertRelation(crypto.randomUUID(), body));
  const createArgument = useWorkbenchMutation(client, workbench, async (body) => client.createExpertArgument(crypto.randomUUID(), body));
  const createGrounding = useWorkbenchMutation(client, workbench, async ({ argumentId, body }) => client.createExpertGrounding(argumentId, crypto.randomUUID(), body));
  const mutations = [createNode, createRelation, createArgument, createGrounding];
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
    </div>

    <article><h3>Current structured model</h3>
      <p>{data.nodes.length} objects · {data.evidence.length} visible evidence fragments · {data.relations.length} typed relations · {data.arguments.length} arguments · {data.grounding_sets.length} grounding sets</p>
      <ul>{data.nodes.map((n) => <li key={n.object_id}><strong>{n.kind}</strong> — {n.text}<br/><small>{n.rationale}</small></li>)}</ul>
      {data.relations.length > 0 && <details><summary>Relations</summary><ul>{data.relations.map((r) => <li key={r.relation_id}>{r.family}/{r.kind}: {r.left_object_id} → {r.right_object_id}</li>)}</ul></details>}
      {data.grounding_sets.length > 0 && <details><summary>Grounding</summary><ul>{data.grounding_sets.map((g) => <li key={g.grounding_set_id}>{g.effect}/{g.adequacy}: {g.members.length} joint member(s)</li>)}</ul></details>}
    </article>
  </section>;
}
