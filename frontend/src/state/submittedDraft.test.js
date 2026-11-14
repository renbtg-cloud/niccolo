import { describe, expect, it } from 'vitest';
import { clearOnlySubmittedText } from './submittedDraft.js';

describe('preserving human draft text after a completed server action', () => {
  it('clears the exact note submitted successfully', () => {
    expect(clearOnlySubmittedText('  checked sources  ', 'checked sources')).toBe('');
    expect(clearOnlySubmittedText(' ', null)).toBe('');
  });
  it('keeps corrections typed while the earlier request was in flight', () => {
    expect(clearOnlySubmittedText('checked sources and found a conflict', 'checked sources'))
      .toBe('checked sources and found a conflict');
    expect(clearOnlySubmittedText('A revised question', 'Original question'))
      .toBe('A revised question');
  });
  it('does not mistake changed whitespace inside a note for the old note', () => {
    expect(clearOnlySubmittedText('a  b', 'a b')).toBe('a  b');
  });
});
