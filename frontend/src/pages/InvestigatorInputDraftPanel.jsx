import { useState } from 'react';
import { useMutation } from '@tanstack/react-query';
import { relationSemantics } from '../state/expertWorkbench.js';

const draftKinds = {
  PROPOSITION: 'Statement',
  HYPOTHESIS: 'Hypothesis',
  ASSUMPTION: 'What-if assumption',
  INFORMATION_GAP: 'Information gap',
  QUESTION: 'Investigative question',
};
const inputLimit = 4096;
const encoder = new TextEncoder();

function draftRationale(suggestion, excerpt) {
  const full = `Draft by ${suggestion.provider} / ${suggestion.model}. Source excerpt: ${excerpt}`;
  if (encoder.encode(full).length <= inputLimit) return full;
  let bounded = '';
  for (const character of full) {
    if (encoder.encode(bounded + character + '…').length > inputLimit) return bounded + '…';
    bounded += character;
  }
  return bounded;
}

export function InvestigatorInputDraftPanel({ client, enabled, onUseObject }) {
  const [text, setText] = useState('');
  const [suggestion, setSuggestion] = useState(null);
  const generate = useMutation({
    mutationFn: async (input) => (await client.draftExpertInput(input)).value,
    onSuccess: setSuggestion,
  });
  const canDraft = text.trim() && encoder.encode(text).length <= inputLimit;
  function submit(event) {
    event.preventDefault();
    if (!canDraft || generate.isPending) return;
    setSuggestion(null);
    generate.mutate(text);
  }
  const { draft } = suggestion ?? {};
  return <article className="review-panel">
    <h3>Draft from investigator text</h3>
    <p>Ask the configured model to propose statements and connections from this text. Review each proposal before recording it in the Workbench.</p>
    {enabled ? <form onSubmit={submit} className="human-context">
      <label>Investigator text<textarea value={text} maxLength={inputLimit} disabled={generate.isPending} onChange={event => setText(event.target.value)}/></label>
      <button type="submit" disabled={!canDraft || generate.isPending}>Propose a draft</button>
    </form> : <p>Configure a model to draft from text. Manual authoring is available below.</p>}
    {generate.isPending && <p role="status">Waiting for the model's draft…</p>}
    {generate.error && <p role="alert" className="error">{generate.error.message}</p>}
    {suggestion && <>
      <h4>Proposed structure for review</h4>
      <p>Suggested by {suggestion.provider} / {suggestion.model}. Use the object form to edit and record individual items.</p>
      <blockquote>{suggestion.input_text}</blockquote>
      {!draft.objects.length && <p>The model returned no case objects to review.</p>}
      <ol>{draft.objects.map((item, index) => <li key={index}>
        <strong>{draftKinds[item.kind]}</strong>
        <p>{item.text}</p>
        <blockquote>{item.source_excerpt}</blockquote>
        <button type="button" onClick={() => onUseObject({
          kind: item.kind,
          text: item.text,
          rationale: draftRationale(suggestion, item.source_excerpt),
        })}>Edit in the object form</button>
      </li>)}</ol>
      {draft.relations.length > 0 && <>
        <h4>Proposed connections</h4>
        <ul>{draft.relations.map((relation, index) => <li key={index}>
          <strong>Object {relation.from + 1} → Object {relation.to + 1}</strong>: {relationSemantics[relation.kind]?.label ?? relation.kind}
          <p>{relation.explanation}</p>
        </li>)}</ul>
      </>}
      <button type="button" onClick={() => setSuggestion(null)}>Discard draft</button>
    </>}
  </article>;
}
