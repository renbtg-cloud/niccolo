import { situationTextError } from './situationText.js';

export function pdfExcerptContext({ document, page, excerpt }) {
  const title = document.trim();
  if (!title) throw new Error('Enter the PDF title or file name.');
  if (!/^[1-9][0-9]{0,5}$/.test(page.trim())) throw new Error('Enter a page number from 1 to 999999.');
  const selected = excerpt.trim();
  if (!selected) throw new Error('Paste the text you selected from the PDF.');
  const oneLine = selected.replace(/\s+/gu, ' ');
  const text = `User transcription from PDF “${title}”, page ${page.trim()} (original unavailable to Niccolò, whitespace normalized): “${oneLine}”`;
  const error = situationTextError(text);
  if (error) throw new Error(error);
  return text;
}
