import { expect, it } from 'vitest';
import { situationTextError } from './situationText.js';

it('applies the server UTF-8 size bound to trimmed text', () => {
  expect(situationTextError(' é'.trim().repeat(2048))).toBeNull();
  expect(situationTextError('é'.repeat(2049))).toMatch(/4096 UTF-8 bytes/);
  expect(situationTextError('   text   ')).toBeNull();
});

it('rejects internal control characters without rejecting accented text', () => {
  expect(situationTextError('ação')).toBeNull();
  expect(situationTextError('a\nb')).toMatch(/line breaks/);
  expect(situationTextError('a\u0085b')).toMatch(/control characters/);
});
