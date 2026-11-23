import { useEffect, useRef, useState } from 'react';
import { MAX_SOURCE_BYTES } from '../state/sourceUpload.js';

export function PdfTextPreview({ client, documents = [], disabled, onSelect }) {
  const active = useRef(null);
  const text = useRef(null);
  const [preview, setPreview] = useState(null);
  const [index, setIndex] = useState(0);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [requestedPage, setRequestedPage] = useState('');
  const [selection, setSelection] = useState(null);

  useEffect(() => () => active.current?.abort(), []);

  async function load(file, stored = null, firstPage = 1, showPage = firstPage) {
    active.current?.abort();
    const sameOriginal = stored && stored.document_id === preview?.documentId;
    if (!sameOriginal) setPreview(null);
    setSelection(null);
    if (!sameOriginal) {
      setIndex(0);
      setRequestedPage('');
    }
    setError('');
    setBusy(false);
    if (!file && !stored) return;
    if (file && (!/\.pdf$/i.test(file.name) || file.size === 0 || file.size > MAX_SOURCE_BYTES)) {
      setError('Select a non-empty PDF of at most 4 MiB.');
      return;
    }
    const request = new AbortController();
    active.current = request;
    setBusy(true);
    try {
      const { value } = stored
        ? await client.previewStoredPdf(stored.document_id, request.signal, firstPage)
        : await client.previewPdf(file, request.signal);
      request.signal.throwIfAborted();
      if (!Array.isArray(value?.pages)) throw new Error('Invalid PDF preview response.');
      const nextIndex = value.pages.findIndex(item => item.number === showPage);
      if (stored && nextIndex < 0) throw new Error(`Page ${showPage} is outside this PDF.`);
      setIndex(Math.max(0, nextIndex));
      setPreview({ ...value, title: stored?.locator ?? file.name, documentId: stored?.document_id, stored, firstPage });
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
    {documents.length > 0 && <label>Or preview an attached PDF
      <select disabled={disabled} value="" onChange={event=>{
        const stored = documents.find(item => item.document_id === event.currentTarget.value);
        if (stored) void load(null, stored);
      }}>
        <option value="">Choose an attached PDF</option>
        {documents.map(item=><option key={item.document_id} value={item.document_id}>{item.locator}</option>)}
      </select>
    </label>}
    <p>Previewing a file does not add it to the source catalog.</p>
    {busy && <p role="status">Extracting PDF text…</p>}
    {error && <p role="alert">{error} You can still paste a passage manually.</p>}
    {preview?.truncated && !preview.stored && <p>Showing the first 20 pages. To browse later pages here, attach the PDF to the Situation.</p>}
    {preview?.stored && <div>
      <form onSubmit={event=>{
        event.preventDefault();
        const target = Number(requestedPage);
        if (!/^[1-9][0-9]{0,5}$/.test(requestedPage)) {
          setError('Enter a page number from 1 to 999999.');
          return;
        }
        const visibleIndex = preview.pages.findIndex(item => item.number === target);
        if (visibleIndex >= 0) {
          setError('');
          selectPage(visibleIndex);
        } else {
          const firstPage = Math.floor((target - 1) / 20) * 20 + 1;
          void load(null, preview.stored, firstPage, target);
        }
      }}>
        <label>Go to page<input inputMode="numeric" value={requestedPage} onChange={event=>setRequestedPage(event.target.value)} disabled={disabled||busy}/></label>
        <button disabled={disabled||busy||!requestedPage}>Go</button>
      </form>
      {preview.firstPage > 1 && <button type="button" disabled={disabled||busy} onClick={()=>void load(null, preview.stored, Math.max(1, preview.firstPage - 20))}>Previous 20 pages</button>}
      {preview.next_page && <button type="button" disabled={disabled||busy} onClick={()=>void load(null, preview.stored, preview.next_page)}>Next 20 pages</button>}
    </div>}
    {preview && !preview.pages.some(item => item.text) &&
      <p>No selectable text was found in the preview. You can describe a page in your own words.</p>}
    {page && <div>
      <button type="button" disabled={busy||index===0} onClick={()=>selectPage(index-1)}>Previous page</button>
      <span role="status">Page {page.number}</span>
      <button type="button" disabled={busy||index+1>=preview.pages.length} onClick={()=>selectPage(index+1)}>Next page</button>
      <label>Page text<textarea ref={text} key={page.number} readOnly rows={8} value={page.text} onSelect={selectText}/></label>
      <button type="button" disabled={!selection||disabled||busy} onClick={()=>{
        const selected = { document: preview.title, page: String(page.number), excerpt: selection, documentId: preview.documentId };
        onSelect({ ...selected, endPage: '', section: '', preview: selected });
      }}>Use selected text in the passage form</button>
      {page.text.length===0 && <small>This page has no extracted text.</small>}
    </div>}
  </div>;
}
