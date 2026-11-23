import { situationTextError } from './situationText.js';

function invalid(field, message) {
  const error = new Error(message);
  error.field = field;
  throw error;
}

export function pdfExcerptContext({ document, page, endPage = "", section = "", excerpt, preview = null }, attachedOriginal = null) {
  const title = document.trim();
  if (!title) invalid('document', 'Enter the PDF title or file name.');
  if (!/^[1-9][0-9]{0,5}$/.test(page.trim())) invalid('page', 'Enter a page number from 1 to 999999.');
  const start = page.trim();
  const end = endPage.trim();
  if (end && (!/^[1-9][0-9]{0,5}$/.test(end) || Number(end) < Number(start))) {
    invalid('endPage', 'Enter a valid last page at or after the first page.');
  }
  const location = end && end !== start ? `pages ${start}–${end}` : `page ${start}`;
  const heading = section.trim();
  const reference = heading ? `${location}, section “${heading}”` : location;
  if (/\p{Cc}/u.test(title)) invalid('document', 'Use a title without control characters.');
  if (/\p{Cc}/u.test(heading)) invalid('section', 'Use a section title without control characters.');
  const selected = excerpt.trim();
  if (!selected) invalid('excerpt', 'Paste the text you selected from the PDF.');
  const oneLine = selected.replace(/\s+/gu, ' ');
  const matchesPreview = preview && preview.excerpt === selected && preview.document === title
    && String(preview.page) === start && (!end || end === start);
  const matchesOriginal = !preview?.documentId || preview.documentId === attachedOriginal?.document_id;
  const origin = matchesPreview && matchesOriginal ? 'User-selected extracted text from PDF' : 'User transcription from PDF';
  const original = attachedOriginal
    ? `stored original ${attachedOriginal.document_id}`
    : preview === null ? 'original unavailable to Niccolò' : 'original not retained by this preview';
  const text = `${origin} “${title}”, ${reference} (${original}, whitespace normalized): “${oneLine}”`;
  const error = situationTextError(text);
  if (error) invalid('excerpt', error);
  return text;
}
