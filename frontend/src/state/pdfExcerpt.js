import { situationTextError } from './situationText.js';

export function pdfExcerptContext({ document, page, endPage = "", section = "", excerpt }) {
  const title = document.trim();
  if (!title) throw new Error('Enter the PDF title or file name.');
  if (!/^[1-9][0-9]{0,5}$/.test(page.trim())) throw new Error('Enter a page number from 1 to 999999.');
  const start = page.trim();
  const end = endPage.trim();
  if (end && (!/^[1-9][0-9]{0,5}$/.test(end) || Number(end) < Number(start))) {
    throw new Error('Enter a valid last page at or after the first page.');
  }
  const location = end && end !== start ? `pages ${start}–${end}` : `page ${start}`;
  const heading = section.trim();
  const reference = heading ? `${location}, section “${heading}”` : location;
  const selected = excerpt.trim();
  if (!selected) throw new Error('Paste the text you selected from the PDF.');
  const oneLine = selected.replace(/\s+/gu, ' ');
  const text = `User transcription from PDF “${title}”, ${reference} (original unavailable to Niccolò, whitespace normalized): “${oneLine}”`;
  const error = situationTextError(text);
  if (error) throw new Error(error);
  return text;
}
