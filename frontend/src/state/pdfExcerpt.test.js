import { expect, it } from 'vitest';
import { pdfExcerptContext } from './pdfExcerpt.js';

const draft = { document: ' Relatório.pdf ', page: '12', excerpt: 'Primeira linha.\n Segunda linha.' };

it('preserves the supplied title and page and identifies the excerpt as a human transcription', () => {
  expect(pdfExcerptContext(draft)).toBe('User transcription from PDF “Relatório.pdf”, page 12 (original unavailable to Niccolò): “Primeira linha. Segunda linha.”');
});

it('requires a title, a page and a passage', () => {
  expect(() => pdfExcerptContext({ ...draft, document: ' ' })).toThrow(/title/);
  expect(() => pdfExcerptContext({ ...draft, page: '0' })).toThrow(/page/);
  expect(() => pdfExcerptContext({ ...draft, excerpt: ' ' })).toThrow(/selected/);
});

it('counts attribution and multibyte text toward the stored context limit', () => {
  expect(() => pdfExcerptContext({ ...draft, excerpt: 'é'.repeat(2048) })).toThrow(/4096 UTF-8 bytes/);
});
