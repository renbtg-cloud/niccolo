import { useEffect, useRef, useState } from 'react';
import { MAX_SOURCE_BYTES } from '../state/sourceUpload.js';

export function PdfTextPreview({ client, disabled, onSelect }) {
  const active = useRef(null);
  const text = useRef(null);
  const [preview, setPreview] = useState(null);
  const [index, setIndex] = useState(0);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [selection, setSelection] = useState(null);

  useEffect(() => () => active.current?.abort(), []);

  async function load(file) {
    active.current?.abort();
    setPreview(null);
    setSelection(null);
    setIndex(0);
    setError('');
    setBusy(false);
    if (!file) return;
    if (!/\.pdf$/i.test(file.name) || file.size === 0 || file.size > MAX_SOURCE_BYTES) {
      setError('Select a non-empty PDF of at most 4 MiB.');
      return;
    }
    const request = new AbortController();
    active.current = request;
    setBusy(true);
    try {
      const { value } = await client.previewPdf(file, request.signal);
      request.signal.throwIfAborted();
      if (!Array.isArray(value?.pages)) throw new Error('Invalid PDF preview response.');
      setPreview({ ...value, title: file.name });
    } catch (error) {
      if (!request.signal.aborted) setError(error.message);
    } finally {
      if (!request.signal.aborted) setBusy(false);
    }
  }

  const page = preview?.pages[index];
  function selectPage(next) {
    setSelection(null);
    setIndex(next);
  }

  function selectText() {
    const editor = text.current;
    const excerpt = editor.value.slice(editor.selectionStart, editor.selectionEnd);
    setSelection(excerpt.trim() || null);
  }

  return <div>
    <label>Preview a PDF on this Niccolò server
      <input type="file" accept=".pdf,application/pdf" disabled={disabled} onChange={event=>{
        const file = event.currentTarget.files?.[0];
        event.currentTarget.value = '';
        void load(file);
      }}/>
    </label>
    <p>The server extracts text for this preview. The PDF itself is kept in your files.</p>
    {busy && <p role="status">Extracting PDF text…</p>}
    {error && <p role="alert">{error} You can still paste a passage manually.</p>}
    {preview?.truncated && <p>Showing the first 20 pages.</p>}
    {preview && !preview.pages.some(item => item.text) &&
      <p>No selectable text was found in the preview. You can describe a page in your own words.</p>}
    {page && <div>
      <button type="button" disabled={index===0} onClick={()=>selectPage(index-1)}>Previous page</button>
      <span role="status">Page {page.number}</span>
      <button type="button" disabled={index+1>=preview.pages.length} onClick={()=>selectPage(index+1)}>Next page</button>
      <label>Page text<textarea ref={text} key={page.number} readOnly rows={8} value={page.text} onSelect={selectText}/></label>
      <button type="button" disabled={!selection||disabled} onClick={()=>{
        const selected = { document: preview.title, page: String(page.number), excerpt: selection };
        onSelect({ ...selected, endPage: '', section: '', preview: selected });
      }}>Use selected text in the passage form</button>
      {page.text.length===0 && <small>This page has no extracted text.</small>}
    </div>}
  </div>;
}
