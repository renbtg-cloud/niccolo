import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';

const cueLabels = { NAME: 'Name', FACE: 'Face', PUBLIC_ROLE: 'Public role' };
const reportLabels = {
  RECOGNITION_REPORTED: 'Recognition reported',
  NON_RECOGNITION_REPORTED: 'Non-recognition reported',
  EXPOSURE_REPORTED: 'Exposure reported',
};

export function PublicProminenceReports({ result, actors, provenance }) {
  const {query, records, truncated} = result;
  const actorLabel = (id) => actors.find((actor) => actor.actor_id === id)?.display_label ?? id;
  const sources = new Map(provenance.map((link) => [
    `${link.object_id}:${link.representation_id}`, link,
  ]));

  return <>
    <p><strong>{actorLabel(query.subject_actor_id)}</strong> · {actorLabel(query.audience_actor_id)}
      {query.cue && ` · ${cueLabels[query.cue]}`}</p>
    <p>On {query.on} · reports available through {query.known_through}</p>
    {result.coverage === 'NO_RECORDED_REPORTS' && <p>No reports recorded for this selection.</p>}
    {truncated && <p role="status">Showing only the first 100 matching reports.</p>}
    {records.length > 0 && <ul>{records.map((record) => {
      const source = sources.get(`${record.claim_object_id}:${record.representation_id}`);
      return <li key={record.record_id}>
        <strong>{reportLabels[record.observation]}</strong> · {cueLabels[record.cue]}
        <p>{record.audience_scope}</p>
        <p>Applies {record.effective_from} to {record.effective_through} · available {record.available_on}</p>
        {source ? <a href={`#source-${record.claim_object_id}-${record.representation_id}`}>
          {source.artifact_label} · {source.scope_locator}
        </a> : <small>Source {record.representation_id}</small>}
      </li>;
    })}</ul>}
  </>;
}

export function PublicProminencePanel({ client, actors, provenance }) {
  const [draft, setDraft] = useState({
    subject_actor_id: '', audience_actor_id: '', cue: '', on: '', known_through: '',
  });
  const [lookup, setLookup] = useState(null);
  const reports = useQuery({
    queryKey: ['public-prominence', client.config.workspace, client.config.situation, lookup],
    enabled: lookup !== null,
    queryFn: async ({ signal }) => (await client.publicProminence(lookup, signal)).value,
  });
  const update = (field) => (event) => setDraft({...draft, [field]: event.target.value});
  const ready = draft.subject_actor_id && draft.audience_actor_id
    && draft.subject_actor_id !== draft.audience_actor_id && draft.on && draft.known_through;

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
        <label>Subject<select value={draft.subject_actor_id} onChange={update('subject_actor_id')} required>
          <option value="">Choose…</option>
          {actors.map((actor) => <option key={actor.actor_id} value={actor.actor_id}>{actor.display_label}</option>)}
        </select></label>
        <label>Audience<select value={draft.audience_actor_id} onChange={update('audience_actor_id')} required>
          <option value="">Choose…</option>
          {actors.map((actor) => <option key={actor.actor_id} value={actor.actor_id}
            disabled={actor.actor_id === draft.subject_actor_id}>{actor.display_label}</option>)}
        </select></label>
        <label>Recognition cue<select value={draft.cue} onChange={update('cue')}>
          <option value="">Any cue</option>
          {Object.entries(cueLabels).map(([cue, label]) => <option key={cue} value={cue}>{label}</option>)}
        </select></label>
        <label>Date being investigated<input type="date" value={draft.on} onChange={update('on')} required/></label>
        <label>Reports available through<input type="date" value={draft.known_through}
          onChange={update('known_through')} required/></label>
        <button disabled={!ready || reports.isFetching}>Find reports</button>
      </form>}
    {reports.isFetching && <p role="status">Looking up reports…</p>}
    {reports.error && <p role="alert">{reports.error.message}</p>}
    {reports.data && <PublicProminenceReports result={reports.data} actors={actors} provenance={provenance}/>}
  </details>;
}
