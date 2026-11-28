import { useEffect, useRef, useState } from 'react';
import { useMutation } from '@tanstack/react-query';
import { SupportRouteDecision } from './SupportRouteDecision.jsx';
import { ExposureOrderReportForm, ExposureOrderReports } from './ExposureOrderReports.jsx';

const dispositionLabels = {
  REVIEW_CANDIDATE: 'The earlier reported exposure makes this detail worth reviewing.',
  EXPOSURE_AFTER_TESTIMONY: 'This reported exposure follows the testimony.',
  ORDER_UNCERTAIN: 'The available information leaves the order uncertain.',
  ORDER_CONFLICT: 'The available sources disagree about which came first.',
  EXPOSURE_NOT_AVAILABLE_AT_CUTOFF: 'The exposure report was unavailable at the chosen cutoff.',
};

export function DetailExposureResult({ result, nodes, children }) {
  const labels = new Map(nodes.map((node) => [node.object_id, node.text]));
  const routes = result.affected_routes;
  return <section aria-label="Exposure review result">
    <p><strong>{dispositionLabels[result.disposition]}</strong></p>
    <p>{labels.get(result.testimony_detail.object_id) ?? result.testimony_detail.object_id}</p>
    {result.needs_finer_timing && <p>Look for a source that establishes which came first.</p>}
    {routes.length > 0 && <>
      <h5>Routes that depend on this detail</h5>
      <ul>{routes.map((route) => <li key={`${route.argument_id}:${route.grounding_set_id}`}>
        <a href={`#workbench-object-${route.conclusion_object_id}`}>
          {labels.get(route.conclusion_object_id) ?? route.conclusion_object_id}
        </a>
        <small> · Grounding set {route.grounding_set_id}</small>
      </li>)}</ul>
    </>}
    {children}
  </section>;
}

export function DetailExposureReview({
  client, exposure, witness, knownThrough, nodes, provenance, groundingSets, routeHistory,
  onRouteChanged, onClose,
}) {
  const panel = useRef(null);
  const currentComparison = useRef(null);
  const [draft, setDraft] = useState({
    testimony_detail: '', candidate_source: '', testimony_from: '', testimony_through: '',
    known_through: knownThrough,
  });
  const review = useMutation({ mutationFn: (body) => client.reviewDetailExposure(body) });
  const [savedOrderDate, setSavedOrderDate] = useState('');
  useEffect(() => {
    panel.current?.focus();
    return () => { currentComparison.current = null; };
  }, []);
  const labels = new Map(nodes.map((node) => [node.object_id, node.text]));
  const sources = new Map(provenance.map((link) => [`${link.object_id}:${link.representation_id}`, link]));
  const testimony = sources.get(draft.testimony_detail);
  const candidate = sources.get(draft.candidate_source);
  const ready = testimony && candidate && testimony.object_id !== candidate.object_id
    && draft.testimony_from && draft.testimony_through && draft.known_through
    && draft.testimony_from <= draft.testimony_through && draft.testimony_through <= draft.known_through;
  function update(field, value) {
    currentComparison.current = null;
    setDraft({...draft, [field]: value});
    setSavedOrderDate('');
    review.reset();
  }
  const sourceOptions = [...sources].map(([key, link]) => <option key={key} value={key}>
    {labels.get(link.object_id)} · {link.artifact_label} · {link.scope_locator}
  </option>);
  const pair = (link) => ({ object_id: link.object_id, representation_id: link.representation_id });

  return <section className="review-panel" ref={panel} tabIndex={-1} aria-label="Testimony detail review">
    <h4>Review a testimony detail for prior exposure</h4>
    <p>Reported exposure for <strong>{witness}</strong> · {exposure.audience_scope}</p>
    <p>Applies {exposure.effective_from} to {exposure.effective_through} · available {exposure.available_on}</p>
    <button type="button" onClick={onClose}>Close detail review</button>
    {new Set(provenance.map((link) => link.object_id)).size > 1 ? <form className="human-context" onSubmit={(event) => {
      event.preventDefault();
      if (!ready || review.isPending) return;
      const input = {
        witness_actor_id: exposure.audience_actor_id, exposure_record_id: exposure.record_id,
        testimony_detail: pair(testimony), candidate_source: pair(candidate),
        testimony_from: draft.testimony_from, testimony_through: draft.testimony_through,
        known_through: draft.known_through,
      };
      currentComparison.current = input;
      review.mutate(input);
    }}>
      <fieldset disabled={review.isPending}>
        <label>Detail in the testimony<select value={draft.testimony_detail}
          onChange={(event) => update('testimony_detail', event.target.value)} required>
          <option value="">Choose a linked passage…</option>{sourceOptions}
        </select></label>
        <label>Candidate origin of the detail<select value={draft.candidate_source}
          onChange={(event) => update('candidate_source', event.target.value)} required>
          <option value="">Choose a linked passage…</option>{sourceOptions}
        </select></label>
        {testimony && candidate && testimony.object_id === candidate.object_id &&
          <p>Choose a separate graph object for the candidate origin.</p>}
        <label>Earliest testimony date<input type="date" value={draft.testimony_from}
          onChange={(event) => update('testimony_from', event.target.value)} required/></label>
        <label>Latest testimony date<input type="date" value={draft.testimony_through} min={draft.testimony_from || undefined}
          onChange={(event) => update('testimony_through', event.target.value)} required/></label>
        <label>Reports available through<input type="date" value={draft.known_through} min={draft.testimony_through || undefined}
          onChange={(event) => update('known_through', event.target.value)} required/></label>
        <button disabled={!ready}>Check possible influence</button>
      </fieldset>
    </form> : <p>Link the testimony detail and its candidate origin to source passages first.</p>}
    {review.error && <p role="alert">{review.error.message}</p>}
    {review.isPending && <p role="status">Checking the order of events…</p>}
    {savedOrderDate && <p role="status">Order report recorded.
      {savedOrderDate > draft.known_through && ` It is available from ${savedOrderDate}, after this review's cutoff.`}
    </p>}
    {review.data && <>
      <DetailExposureResult result={review.data.value} nodes={nodes}>
        <ExposureOrderReports result={review.data.value} provenance={provenance}/>
        <DetailExposureRouteDecisions result={review.data.value} input={review.variables} client={client}
          groundingSets={groundingSets} routeHistory={routeHistory} onSaved={onRouteChanged}/>
      </DetailExposureResult>
      {review.data.value.exposure && <ExposureOrderReportForm client={client} review={review.variables}
        sources={sources} sourceOptions={sourceOptions} onSaved={(date, comparison) => {
          if (currentComparison.current !== comparison) return;
          setSavedOrderDate(date);
          review.mutate(comparison);
        }}/>}
    </>}
  </section>;
}

function sameRoute(left, right) {
  return left.argument_id === right.argument_id && left.grounding_set_id === right.grounding_set_id;
}

export function DetailExposureRouteDecisions({ result, input, client, groundingSets, routeHistory, onSaved }) {
  if (!result.detail_needs_review) return null;
  const eligible = groundingSets.filter((grounding) => grounding.effect === 'SUPPORTS'
    && result.affected_routes.some((route) => sameRoute(grounding, route)));
  if (eligible.length === 0) return null;

  return <details>
    <summary>Decide whether to use an affected support route</summary>
    <ul>{eligible.map((route) => {
      const history = routeHistory.filter((decision) => sameRoute(decision.route, route));
      return <SupportRouteDecision key={`${route.argument_id}:${route.grounding_set_id}`}
        route={route} label={route.rationale} suspended={history.at(-1)?.disposition === 'SUSPEND'}
        history={history} onSaved={onSaved}
        onRecord={(eventId, decision) => client.recordDetailExposureRouteUse(eventId, {
          ...decision, review: input, expected_review_sha256: result.review_sha256,
        })}/>;
    })}</ul>
  </details>;
}
