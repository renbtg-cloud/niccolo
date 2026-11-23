import { PdfTextPreview } from './PdfTextPreview.jsx';
import { useId, useState } from 'react';
import { pdfExcerptContext } from '../state/pdfExcerpt.js';

export function PdfExcerptForm({ client, documents, onPrepare, onDraftChange, disabled, saving, previewOutdated }) {
  const [draft, setDraft] = useState({ document: '', page: '', endPage: '', section: '', excerpt: '' });
  const original = documents.find(item => item.document_id === draft.documentId);
  const [error, setError] = useState(null);
  const errorId = useId();

  function change(key, value) {
    onDraftChange?.();
    setDraft(current => ({ ...current, [key]: value }));
    setError(null);
  }

  function prepare(event) {
    event.preventDefault();
    if (disabled) return;
    let context;
    try {
      if (draft.documentId && !original) {
        throw Object.assign(new Error('This PDF is no longer attached. Choose a reference again.'), { field: 'original' });
      }
      context = pdfExcerptContext(draft, original);
    } catch (validationError) {
      setError(validationError);
      event.currentTarget.elements.namedItem(validationError.field)?.focus();
      return;
    }
    onPrepare(context);
    setError(null);
  }

  function field(name) {
    return {
      name,
      'aria-invalid': error?.field === name || undefined,
      'aria-describedby': error?.field === name ? errorId : undefined,
    };
  }

  return <details>
    <summary>Use a passage from a PDF</summary>
    <p>Open the original PDF yourself. Copy the passage you want Niccolò to consider and record its page. The prepared text combines lines into a single paragraph.</p>
    <PdfTextPreview client={client} documents={documents} disabled={saving} onSelect={selection=>{
      onDraftChange?.();
      setDraft(selection);
      setError(null);
    }}/>
    <form onSubmit={prepare}>
      <fieldset disabled={saving}>
      {documents.length > 0 || draft.documentId ? <label>Original for this passage
        <select {...field('original')} value={draft.documentId ?? ''} onChange={event=>{
          const selected = documents.find(item => item.document_id === event.target.value);
          onDraftChange?.();
          setDraft(current => ({
            ...current,
            documentId: selected?.document_id,
            document: selected?.locator ?? current.document,
          }));
          setError(null);
        }}>
          <option value="">Use a manually entered reference</option>
          {draft.documentId && !original && <option value={draft.documentId}>Previously attached PDF unavailable</option>}
          {documents.map(item => <option key={item.document_id} value={item.document_id}>{item.locator}</option>)}
        </select>
      </label> : null}
      <label>PDF title or file name<input {...field('document')} value={draft.document} readOnly={Boolean(original)} onChange={event=>change('document',event.target.value)} required/></label>
      <label>First page in your PDF reader<input inputMode="numeric" {...field('page')} value={draft.page} onChange={event=>change('page',event.target.value)} required/></label>
      <label>Last page, if different<input inputMode="numeric" {...field('endPage')} value={draft.endPage} onChange={event=>change('endPage',event.target.value)}/></label>
      <label>Section title (optional)<input {...field('section')} value={draft.section} onChange={event=>change('section',event.target.value)}/></label>
      <label><input type="checkbox" checked={Boolean(draft.description)} onChange={event=>change('description',event.target.checked)}/> Describe something on the page in my own words</label>
      <label>{draft.description ? 'Your description' : 'Selected passage'}<textarea {...field('excerpt')} value={draft.excerpt} onChange={event=>change('excerpt',event.target.value)} required/></label>
      {disabled && !saving && <p>Save or clear the current human-context draft before preparing another passage.</p>}
      {previewOutdated && <p role="status">Prepare again to update the transcription preview.</p>}
      <button type="submit" disabled={disabled}>Prepare as human context</button>
      {error && <p id={errorId} role="alert">{error.message}</p>}
      </fieldset>
    </form>
  </details>;
}
