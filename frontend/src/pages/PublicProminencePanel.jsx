import { useRef, useState } from 'react';
import { useMutation, useQuery } from '@tanstack/react-query';
import { DetailExposureReview } from './DetailExposureReview.jsx';

const cueLabels = { NAME: 'Name', FACE: 'Face', PUBLIC_ROLE: 'Public role' };
const reportLabels = {
  RECOGNITION_REPORTED: 'Recognition reported',
  NON_RECOGNITION_REPORTED: 'Non-recognition reported',
  EXPOSURE_REPORTED: 'Exposure reported',
};
const encoder = new TextEncoder();
const emptyReport = {
  subject_actor_id: '', audience_actor_id: '', cue: '', observation: '',
  effective_from: '', effective_through: '', available_on: '', audience_scope: '', source: '',
};

function ActorSelect({ label, actors, value, onChange, excluded }) {
  return <label>{label}<select value={value} onChange={onChange} required>
    <option value="">Choose…</option>
    {actors.map((actor) => <option key={actor.actor_id} value={actor.actor_id}
      disabled={actor.actor_id === excluded}>{actor.display_label}</option>)}
  </select></label>;
}

function dateRangeIsValid(from, through) {
  return from && through && from <= through;
}

export function PublicProminenceRecordForm({ client, actors, nodes, provenance, onRecorded }) {
  const [draft, setDraft] = useState(emptyReport);
  const pending = useRef(null);
  const propositions = new Map(nodes.filter((node) => node.kind === 'PROPOSITION').map(
    (node) => [node.object_id, node.text],
  ));
  const sources = new Map(provenance.filter((link) => propositions.has(link.object_id)).map(
    (link) => [`${link.object_id}:${link.representation_id}`, link],
  ));
  const record = useMutation({
    mutationFn: (body) => {
      const signature = JSON.stringify(body);
      if (pending.current?.signature !== signature) {
        pending.current = { signature, id: crypto.randomUUID() };
      }
      return client.recordPublicProminence(pending.current.id, body);
    },
    onSuccess: async () => {
      pending.current = null;
      setDraft(emptyReport);
      await onRecorded();
    },
  });
  const source = sources.get(draft.source);
  const audienceScope = draft.audience_scope.trim();
  const update = (field) => (event) => setDraft({...draft, [field]: event.target.value});
  const datesOptional = draft.observation === 'EXPOSURE_REPORTED';
  const datesValid = dateRangeIsValid(draft.effective_from, draft.effective_through)
    || (datesOptional && !draft.effective_from && !draft.effective_through);
  const ready = draft.subject_actor_id && draft.audience_actor_id
    && draft.subject_actor_id !== draft.audience_actor_id && draft.cue && draft.observation
    && datesValid && draft.available_on
    && audienceScope && encoder.encode(audienceScope).length <= 2048 && source;

  return <details>
    <summary>Record a recognizability report</summary>
    {sources.size === 0 ? <p>Link a description or transcription to a proposition before recording a report.</p> :
      <form className="human-context" onSubmit={(event) => {
        event.preventDefault();
        if (!ready || record.isPending) return;
        record.mutate({
          subject_actor_id: draft.subject_actor_id, audience_actor_id: draft.audience_actor_id,
          cue: draft.cue, observation: draft.observation, audience_scope: audienceScope,
          effective_from: draft.effective_from || null, effective_through: draft.effective_through || null,
          available_on: draft.available_on,
          claim_object_id: source.object_id, representation_id: source.representation_id,
        });
      }}>
        <fieldset disabled={record.isPending}>
          <ActorSelect label="Subject" actors={actors} value={draft.subject_actor_id}
            onChange={update('subject_actor_id')}/>
          <ActorSelect label="Audience" actors={actors} value={draft.audience_actor_id}
            onChange={update('audience_actor_id')} excluded={draft.subject_actor_id}/>
          <label>Recognition cue<select value={draft.cue} onChange={update('cue')} required>
            <option value="">Choose…</option>
            {Object.entries(cueLabels).map(([cue, label]) => <option key={cue} value={cue}>{label}</option>)}
          </select></label>
          <label>What the report says<select value={draft.observation} onChange={update('observation')} required>
            <option value="">Choose…</option>
            {Object.entries(reportLabels).map(([value, label]) => <option key={value} value={value}>{label}</option>)}
          </select></label>
          <label>Who in the audience the source describes<textarea value={draft.audience_scope}
            onChange={update('audience_scope')} maxLength={2048} required/></label>
          {datesOptional && <p>Leave both event dates blank if the source gives no date for the exposure.</p>}
          <label>Applies from<input type="date" value={draft.effective_from}
            onChange={update('effective_from')} required={!datesOptional}/></label>
          <label>Applies through<input type="date" value={draft.effective_through} min={draft.effective_from || undefined}
            onChange={update('effective_through')} required={!datesOptional}/></label>
          <label>Report available on<input type="date" value={draft.available_on} onChange={update('available_on')} required/></label>
          <label>Linked source passage<select value={draft.source} onChange={update('source')} required>
            <option value="">Choose…</option>
            {[...sources].map(([key, link]) => <option key={key} value={key}>
              {propositions.get(link.object_id)} · {link.artifact_label} · {link.scope_locator}
            </option>)}
          </select></label>
          <button disabled={!ready}>Record report</button>
        </fieldset>
      </form>}
    {record.error && <p role="alert">{record.error.message}</p>}
    {record.isSuccess && <p role="status">Report recorded.</p>}
  </details>;
}

export function PublicProminenceReports({ result, actors, provenance, onReview }) {
  const {query, records, truncated} = result;
  const actorLabel = (id) => actors.find((actor) => actor.actor_id === id)?.display_label ?? id;
  const sources = new Map(provenance.map((link) => [
    `${link.object_id}:${link.representation_id}`, link,
  ]));

  return <>
    <p><strong>{actorLabel(query.subject_actor_id)}</strong> · {actorLabel(query.audience_actor_id)}
      {query.cue && ` · ${cueLabels[query.cue]}`}</p>
    <p>{query.on ? `On ${query.on}` : 'Exposure reports without an event date'} · reports available through {query.known_through}</p>
    {result.coverage === 'NO_RECORDED_REPORTS' && <p>No reports recorded for this selection.</p>}
    {truncated && <p role="status">Showing only the first 100 matching reports.</p>}
    {records.length > 0 && <ul>{records.map((record) => {
      const source = sources.get(`${record.claim_object_id}:${record.representation_id}`);
      return <li key={record.record_id}>
        <strong>{reportLabels[record.observation]}</strong> · {cueLabels[record.cue]}
        <p>{record.audience_scope}</p>
        <p>{record.effective_from ? `Applies ${record.effective_from} to ${record.effective_through}`
          : 'Exposure date unknown'} · available {record.available_on}</p>
        {source ? <a href={`#source-${record.claim_object_id}-${record.representation_id}`}>
          {source.artifact_label} · {source.scope_locator}
        </a> : <small>Source {record.representation_id}</small>}
        {record.observation === 'EXPOSURE_REPORTED' && onReview && <p>
          <button type="button" onClick={() => onReview(record, query.known_through)}>Review a testimony detail</button>
        </p>}
      </li>;
    })}</ul>}
  </>;
}

export function PublicProminencePanel({ client, actors, nodes, provenance, groundingSets, routeHistory, onRouteChanged }) {
  const [draft, setDraft] = useState({
    subject_actor_id: '', audience_actor_id: '', cue: '', on: '', known_through: '', undated: false,
  });
  const [lookup, setLookup] = useState(null);
  const [review, setReview] = useState(null);
  const reports = useQuery({
    queryKey: ['public-prominence', client.config.workspace, client.config.situation, lookup],
    enabled: lookup !== null,
    queryFn: async ({ signal }) => {
      const { undated, ...criteria } = lookup;
      return (await client.publicProminence({...criteria, on: undated ? null : criteria.on}, signal)).value;
    },
  });
  const update = (field) => (event) => setDraft({...draft, [field]: event.target.value});
  const ready = draft.subject_actor_id && draft.audience_actor_id
    && draft.subject_actor_id !== draft.audience_actor_id && (draft.undated || draft.on) && draft.known_through;

  return <details className="review-panel">
    <summary>Recognizability by audience</summary>
    <p>Find attributed reports about who recognized or encountered a subject's name, face or public role.</p>
    {actors.length < 2 ? <p>Register a subject and an audience in the actor registry.</p> :
      <form className="human-context" onSubmit={(event) => {
        event.preventDefault();
        if (!ready) return;
        if (lookup === draft) void reports.refetch();
        else setLookup(draft);
      }}>
        <ActorSelect label="Subject" actors={actors} value={draft.subject_actor_id}
          onChange={update('subject_actor_id')}/>
        <ActorSelect label="Audience" actors={actors} value={draft.audience_actor_id}
          onChange={update('audience_actor_id')} excluded={draft.subject_actor_id}/>
        <label>Recognition cue<select value={draft.cue} onChange={update('cue')}>
          <option value="">Any cue</option>
          {Object.entries(cueLabels).map(([cue, label]) => <option key={cue} value={cue}>{label}</option>)}
        </select></label>
        <label><input type="checkbox" checked={draft.undated}
          onChange={(event) => setDraft({...draft, undated: event.target.checked})}/>
          Find exposures with unknown dates</label>
        <label>Date being investigated<input type="date" value={draft.on} onChange={update('on')}
          required={!draft.undated} disabled={draft.undated}/></label>
        <label>Reports available through<input type="date" value={draft.known_through}
          onChange={update('known_through')} required/></label>
        <button disabled={!ready || reports.isFetching}>Find reports</button>
      </form>}
    {reports.isFetching && <p role="status">Looking up reports…</p>}
    {reports.error && <p role="alert">{reports.error.message}</p>}
    {reports.data && <PublicProminenceReports result={reports.data} actors={actors} provenance={provenance}
      onReview={(record, knownThrough) => setReview({ record, knownThrough })}/>}
    {review && <DetailExposureReview
      key={`${review.record.record_id}:${review.knownThrough}`} client={client} exposure={review.record}
      knownThrough={review.knownThrough} nodes={nodes} provenance={provenance}
      groundingSets={groundingSets} routeHistory={routeHistory} onRouteChanged={onRouteChanged}
      witness={actors.find((actor) => actor.actor_id === review.record.audience_actor_id)?.display_label
        ?? review.record.audience_actor_id} onClose={() => setReview(null)}/>}
    {actors.length >= 2 && <PublicProminenceRecordForm client={client} actors={actors} nodes={nodes}
      provenance={provenance} onRecorded={() => lookup && reports.refetch()}/>}
  </details>;
}
