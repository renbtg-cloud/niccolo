import { PdfTextPreview } from './PdfTextPreview.jsx';
import { useId, useState } from 'react';
import { pdfExcerptContext } from '../state/pdfExcerpt.js';

export function PdfExcerptForm({ client, onPrepare, onDraftChange, disabled, saving, previewOutdated }) {
  const [draft, setDraft] = useState({ document: '', page: '', endPage: '', section: '', excerpt: '' });
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
      context = pdfExcerptContext(draft);
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
    <PdfTextPreview client={client} disabled={saving} onSelect={selection=>{
      onDraftChange?.();
      setDraft(selection);
      setError(null);
    }}/>
    <form onSubmit={prepare}>
      <fieldset disabled={saving}>
      <label>PDF title or file name<input {...field('document')} value={draft.document} onChange={event=>change('document',event.target.value)} required/></label>
      <label>First page in your PDF reader<input inputMode="numeric" {...field('page')} value={draft.page} onChange={event=>change('page',event.target.value)} required/></label>
      <label>Last page, if different<input inputMode="numeric" {...field('endPage')} value={draft.endPage} onChange={event=>change('endPage',event.target.value)}/></label>
      <label>Section title (optional)<input {...field('section')} value={draft.section} onChange={event=>change('section',event.target.value)}/></label>
      <label>Selected passage<textarea {...field('excerpt')} value={draft.excerpt} onChange={event=>change('excerpt',event.target.value)} required/></label>
      {disabled && !saving && <p>Save or clear the current human-context draft before preparing another passage.</p>}
      {previewOutdated && <p role="status">Prepare again to update the transcription preview.</p>}
      <button type="submit" disabled={disabled}>Prepare as human context</button>
      {error && <p id={errorId} role="alert">{error.message}</p>}
      </fieldset>
    </form>
  </details>;
}
