import { useId, useState } from 'react';
import { pdfExcerptContext } from '../state/pdfExcerpt.js';

export function PdfExcerptForm({ onPrepare, disabled }) {
  const [draft, setDraft] = useState({ document: '', page: '', excerpt: '' });
  const [error, setError] = useState('');
  const errorId = useId();

  function change(key, value) {
    setDraft(current => ({ ...current, [key]: value }));
    setError('');
  }

  function prepare(event) {
    event.preventDefault();
    if (disabled) return;
    try {
      const context = pdfExcerptContext(draft);
      onPrepare(context);
      setError('');
    } catch (validationError) {
      setError(validationError.message);
    }
  }

  return <details>
    <summary>Use a passage from a PDF</summary>
    <p>Open the original PDF yourself. Copy the passage you want Niccolò to consider and record its page. The prepared text combines lines into a single paragraph.</p>
    <form onSubmit={prepare} aria-describedby={error ? errorId : undefined}>
      <label>PDF title or file name<input value={draft.document} onChange={event=>change('document',event.target.value)} required/></label>
      <label>Page number<input inputMode="numeric" value={draft.page} onChange={event=>change('page',event.target.value)} required/></label>
      <label>Selected passage<textarea value={draft.excerpt} onChange={event=>change('excerpt',event.target.value)} required/></label>
      {disabled && <p>Save or clear the current human-context draft before preparing another passage.</p>}
      <button type="submit" disabled={disabled}>Prepare as human context</button>
      {error && <p id={errorId} role="alert">{error}</p>}
    </form>
  </details>;
}
