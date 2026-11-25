import { useState } from 'react';
import { useMutation, useQuery } from '@tanstack/react-query';
import { qk } from '../api/keys.js';

const encoder = new TextEncoder();

export function WorkbenchArtifactSources({ client, nodes, links, onLinked }) {
  const [open, setOpen] = useState(false);
  const [object, setObject] = useState('');
  const [representation, setRepresentation] = useState('');
  const [rationale, setRationale] = useState('');
  const view = useQuery({
    queryKey: qk.artifacts(client.config),
    queryFn: async ({ signal }) => (await client.artifacts(signal)).value,
    enabled: open,
  });
  const record = useMutation({
    mutationFn: () => client.linkExpertArtifactProvenance(object, representation, rationale.trim()),
    onSuccess: async () => {
      setRepresentation('');
      setRationale('');
      await onLinked();
    },
  });
  const artifacts = new Map(view.data?.artifacts.map((item) => [item.artifact_id, item.label]));
  const representations = new Map(view.data?.representations.map((item) => [item.representation_id, item]));
  const available = Array.from(representations.values()).filter((item) => !links.some(
    (link) => link.object_id === object && link.representation_id === item.representation_id,
  ));
  const ready = object && available.some((item) => item.representation_id === representation)
    && rationale.trim() && encoder.encode(rationale.trim()).length <= 4096;
  const selected = representations.get(representation);

  return <details className="review-panel" onToggle={(event) => setOpen(event.currentTarget.open)}>
    <summary>Link external artifact provenance{links.length > 0 && ` (${links.length} recorded)`}</summary>
    {view.isLoading && <p role="status">Loading descriptions…</p>}
    {view.error && <p role="alert" className="error">{view.error.message}</p>}
    {view.isSuccess && representations.size === 0 && <p>Record a description or transcription under External artifacts in the Situation first.</p>}
    {representations.size > 0 && <form className="human-context" onSubmit={(event) => {
      event.preventDefault();
      if (ready && !record.isPending) record.mutate();
    }}>
      <label>Graph object<select value={object} disabled={record.isPending} onChange={(event) => {
        setObject(event.target.value);
        setRepresentation('');
      }}>
        <option value="">Choose…</option>
        {nodes.map((node) => <option value={node.object_id} key={node.object_id}>{node.text}</option>)}
      </select></label>
      <label>Supplied description or transcription<select value={representation} disabled={record.isPending || !object}
        onChange={(event) => setRepresentation(event.target.value)}>
        <option value="">Choose…</option>
        {available.map((item) => <option value={item.representation_id} key={item.representation_id}>
          {artifacts.get(item.artifact_id)} · {item.scope_locator} · {item.content.slice(0, 110)}
        </option>)}
      </select></label>
      {selected && <blockquote>{selected.content}</blockquote>}
      <label>Why this material is relevant to the object<textarea maxLength={4096} value={rationale}
        disabled={record.isPending} onChange={(event) => setRationale(event.target.value)}/></label>
      <button disabled={!ready || record.isPending}>Record provenance link</button>
    </form>}
    {record.error && <p role="alert" className="error">{record.error.message}</p>}
    {record.isSuccess && <p role="status">Provenance link recorded.</p>}
    {links.length > 0 && <ul>{links.map((link) => {
      const original = representations.get(link.representation_id);
      return <li key={`${link.object_id}:${link.representation_id}`} id={`source-${link.object_id}-${link.representation_id}`}>
        <a href={`#workbench-object-${link.object_id}`}>{nodes.find((node) => node.object_id === link.object_id)?.text}</a>
        <p>{link.artifact_label} · {link.scope_locator}{link.qualification && ` · ${link.qualification}`}</p>
        {original && <><blockquote>{original.content}</blockquote><small>Supplied by {original.producer_label || original.recorded_by}</small></>}
        <p>{link.rationale}</p><small>Linked by {link.recorded_by}</small>
      </li>;
    })}</ul>}
  </details>;
}
