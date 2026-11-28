import { useRef, useState } from 'react';
import { useMutation } from '@tanstack/react-query';

const directions = {
  EXPOSURE_BEFORE_TESTIMONY: 'Exposure before testimony',
  EXPOSURE_AFTER_TESTIMONY: 'Exposure after testimony',
};

export function ExposureOrderReports({ result, provenance }) {
  const reports = result.order_reports ?? [];
  if (!result.date_order && reports.length === 0) return null;
  const sources = new Map(provenance.map((link) => [`${link.object_id}:${link.representation_id}`, link]));

  return <details open>
    <summary>Basis for the order of events</summary>
    {result.date_order && <p>By calendar dates · {directions[result.date_order]}</p>}
    {reports.length > 0 && <ul>{reports.map((report) => {
      const { object_id, representation_id } = report.source;
      const source = sources.get(`${object_id}:${representation_id}`);
      return <li key={report.report_id}>
        <strong>{directions[report.direction]}</strong> · available {report.available_on}
        <p>{source ? <a href={`#source-${object_id}-${representation_id}`}>
          {source.artifact_label} · {source.scope_locator}
        </a> : <small>Source {representation_id}</small>}</p>
        <small>Recorded by {report.recorded_by}</small>
      </li>;
    })}</ul>}
    {result.order_reports_truncated && <p role="status">Showing at most 50 reports in each direction.</p>}
  </details>;
}

export function ExposureOrderReportForm({ client, review, sources, sourceOptions, onSaved }) {
  const [direction, setDirection] = useState('');
  const [sourceKey, setSourceKey] = useState('');
  const [availableOn, setAvailableOn] = useState('');
  const pending = useRef(null);
  const record = useMutation({
    mutationFn: (body) => {
      const signature = JSON.stringify(body);
      if (pending.current?.signature !== signature) {
        pending.current = { signature, id: crypto.randomUUID() };
      }
      return client.recordExposureOrder(pending.current.id, body);
    },
    onSuccess: (_, body) => onSaved(body.available_on, review),
  });
  const source = sources.get(sourceKey);
  const ready = direction && source && availableOn;

  return <details>
    <summary>Record a source that states which came first</summary>
    <form className="human-context" onSubmit={(event) => {
      event.preventDefault();
      if (!ready || record.isPending) return;
      record.mutate({
        exposure_record_id: review.exposure_record_id,
        testimony_representation_id: review.testimony_detail.representation_id,
        candidate_representation_id: review.candidate_source.representation_id,
        source: { object_id: source.object_id, representation_id: source.representation_id },
        direction,
        available_on: availableOn,
      });
    }}>
      <fieldset disabled={record.isPending}>
        <label>Order stated by the source<select value={direction}
          onChange={(event) => setDirection(event.target.value)} required>
          <option value="">Choose…</option>
          {Object.entries(directions).map(([value, label]) => <option key={value} value={value}>{label}</option>)}
        </select></label>
        <label>Passage that states the order<select value={sourceKey}
          onChange={(event) => setSourceKey(event.target.value)} required>
          <option value="">Choose a linked passage…</option>{sourceOptions}
        </select></label>
        <label>Report available on<input type="date" value={availableOn}
          onChange={(event) => setAvailableOn(event.target.value)} required/></label>
        <button disabled={!ready}>Record order report</button>
      </fieldset>
    </form>
    {record.error && <p role="alert">{record.error.message}</p>}
  </details>;
}
