import { expect, it } from 'vitest';
import { pdfExcerptContext } from './pdfExcerpt.js';

const draft = { document: ' Relatório.pdf ', page: '12', excerpt: 'Primeira linha.\n Segunda linha.' };

it('preserves the supplied title and page and identifies the excerpt as a human transcription', () => {
  expect(pdfExcerptContext(draft)).toBe('User transcription from PDF “Relatório.pdf”, page 12 (original unavailable to Niccolò, whitespace normalized): “Primeira linha. Segunda linha.”');
});

it('requires a title, a page and a passage', () => {
  expect(() => pdfExcerptContext({ ...draft, document: ' ' })).toThrow(/title/);
  expect(() => pdfExcerptContext({ ...draft, page: '0' })).toThrow(/page/);
  expect(() => pdfExcerptContext({ ...draft, excerpt: ' ' })).toThrow(/selected/);
});

it('counts attribution and multibyte text toward the stored context limit', () => {
  expect(() => pdfExcerptContext({ ...draft, excerpt: 'é'.repeat(2048) })).toThrow(/4096 UTF-8 bytes/);
});

it('records a range and heading while rejecting reversed and malformed ranges', () => {
  expect(pdfExcerptContext({ ...draft, endPage: '14', section: ' Resultados ' }))
    .toContain('pages 12–14, section “Resultados”');
  expect(pdfExcerptContext({ ...draft, endPage: '12' })).toContain('page 12');
  expect(() => pdfExcerptContext({ ...draft, endPage: '11' })).toThrow(/last page/);
  expect(() => pdfExcerptContext({ ...draft, endPage: '1e3' })).toThrow(/last page/);
});

it('identifies the affected field when a locator or passage is invalid', () => {
  for (const [overrides, field] of [
    [{document: ' '}, 'document'],
    [{page: '0'}, 'page'],
    [{endPage: '9'}, 'endPage'],
    [{section: 'Heading\u0000'}, 'section'],
    [{excerpt: 'é'.repeat(2048)}, 'excerpt'],
  ]) {
    expect(() => pdfExcerptContext({...draft, ...overrides})).toThrow(expect.objectContaining({field}));
  }
});

it('distinguishes unchanged extractor text from a human-edited passage', () => {
  const selected = { ...draft, preview: { document: draft.document.trim(), page: draft.page, excerpt: draft.excerpt.trim() } };
  expect(pdfExcerptContext(selected)).toMatch(/^User-selected extracted text from PDF/);
  expect(pdfExcerptContext(selected)).toContain('original not retained by Niccolò');
  expect(pdfExcerptContext({ ...selected, excerpt: 'Edited passage' })).toMatch(/^User transcription from PDF/);
  expect(pdfExcerptContext({ ...selected, page: '13' })).toMatch(/^User transcription from PDF/);
  expect(pdfExcerptContext({ ...selected, endPage: '13' })).toMatch(/^User transcription from PDF/);
  expect(pdfExcerptContext({ ...selected, document: 'Another.pdf' })).toMatch(/^User transcription from PDF/);
});
