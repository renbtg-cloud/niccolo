import { useState } from 'react';
import { useMutation } from '@tanstack/react-query';

const maxObjects = 16;

function CitedBasis({ keys, basis }) {
  return <ul>{keys.map((key, position) => {
    const reference = basis.find((item) => item.key === key);
    return <li key={position}>
      {reference.description}{' '}
      {reference.object_ids.map((id, i) => <a key={id} href={`#workbench-object-${id}`}>Open object{i ? ` ${i + 1}` : ''}{' '}</a>)}
    </li>;
  })}</ul>;
}

export function InvestigationSuggestions({ result, onEditCandidate }) {
  return <>
    <h4>Investigation suggestions</h4>
    <p>Suggested by {result.provider} / {result.model}. Review each item before recording it.</p>
    {!result.suggestions.length && <p>The model found no suggestions in this selection.</p>}
    <ol>{result.suggestions.map((item, index) => <li key={index}>
      <strong>{item.kind === 'QUESTION' ? 'Question' : 'What-if'}:</strong> {item.text}
      <p>{item.why_it_matters}</p>
      <CitedBasis keys={item.references} basis={result.basis}/>
      <button type="button" onClick={() => onEditCandidate({
        kind: item.kind === 'QUESTION' ? 'QUESTION' : 'ASSUMPTION',
        text: item.text,
        rationale: `Suggested by ${result.provider} / ${result.model}. ${item.why_it_matters}`,
      })}>Edit before recording</button>
    </li>)}</ol>
  </>;
}

export function WorkbenchProsePanel({ client, nodes, onEditCandidate }) {
  const [selectedIds, setSelectedIds] = useState([]);
  const [language, setLanguage] = useState('');
  const [text, setText] = useState('');
  const describe = useMutation({
    mutationFn: async ({ ids, language }) => (await client.describeExpertSelection(ids, language || null)).value,
    onSuccess: (result) => setText(result.prose.paragraphs.map((paragraph) => paragraph.text).join('\n\n')),
  });
  const suggest = useMutation({
    mutationFn: async ({ ids, language }) => (await client.suggestExpertInvestigation(ids, language || null)).value,
  });
  const busy = describe.isPending || suggest.isPending;
  const ids = nodes.filter((node) => selectedIds.includes(node.object_id)).map((node) => node.object_id);
  const original = describe.data?.prose.paragraphs.map((paragraph) => paragraph.text).join('\n\n');
  function download() {
    const url = URL.createObjectURL(new Blob([text], { type: 'text/plain;charset=utf-8' }));
    const link = document.createElement('a');
    link.href = url;
    link.download = 'niccolo-report-draft.txt';
    link.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  }
  return <details className="review-panel">
    <summary>Describe or investigate selected objects</summary>
    <p>Select up to {maxObjects} objects. The configured model receives their text and the recorded relationships between them.</p>
    <form className="human-context" onSubmit={(event) => {
      event.preventDefault();
      if (!ids.length || ids.length > maxObjects || busy) return;
      describe.mutate({ ids, language });
    }}>
      <fieldset disabled={busy}>
        <legend>Objects to describe</legend>
        {nodes.map((node) => <label key={node.object_id}>
          <input type="checkbox" checked={ids.includes(node.object_id)}
            disabled={ids.length >= maxObjects && !ids.includes(node.object_id)}
            onChange={(event) => setSelectedIds(event.target.checked ? [...ids, node.object_id] : ids.filter((id) => id !== node.object_id))}/>
          {node.text}
        </label>)}
      </fieldset>
      <label>Language<select value={language} onChange={(event) => setLanguage(event.target.value)}>
        <option value="">Use the language of the selection</option>
        <option value="pt-BR">Português</option>
        <option value="en">English</option>
        <option value="es">Español</option>
      </select></label>
      <button type="submit" disabled={!ids.length || busy}>Describe selection</button>
      <button type="button" disabled={!ids.length || busy} onClick={() => suggest.mutate({ ids, language })}>Suggest questions and what-ifs</button>
    </form>
    {describe.isPending && <p role="status">Waiting for description…</p>}
    {describe.error && <p role="alert" className="error">{describe.error.message}</p>}
    {describe.data && <>
      <p>Draft by {describe.data.provider} / {describe.data.model}.</p>
      <label className="human-context">Report draft<textarea value={text} onChange={(event) => setText(event.target.value)}/></label>
      <button type="button" disabled={!text.trim()} onClick={download}>Download text</button>
      <details>
        <summary>{text === original ? 'Basis cited by the draft' : 'Basis cited by the original draft, before edits'}</summary>
        {describe.data.prose.paragraphs.map((paragraph, index) => <div key={index}>
          <p>{paragraph.text}</p>
          <CitedBasis keys={paragraph.references} basis={describe.data.basis}/>
        </div>)}
      </details>
    </>}
    {suggest.isPending && <p role="status">Waiting for investigation suggestions…</p>}
    {suggest.error && <p role="alert" className="error">{suggest.error.message}</p>}
    {suggest.data && <InvestigationSuggestions result={suggest.data} onEditCandidate={onEditCandidate}/>}
  </details>;
}
