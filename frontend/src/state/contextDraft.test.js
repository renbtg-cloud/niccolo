import { describe, expect, it } from 'vitest';
import { afterSuccessfulContextSubmission } from './contextDraft.js';

describe('human-context draft preservation', () => {
  const submitted = { kind: 'user_reported', text: 'Original observation' };
  it('clears only the draft that was actually saved', () => {
    expect(afterSuccessfulContextSubmission({ ...submitted }, submitted)).toEqual({
      ...submitted, text: '',
    });
    expect(afterSuccessfulContextSubmission({ kind: 'user_reported', text: '  Original observation  ' }, submitted).text).toBe('');
  });
  it('preserves edits made while the request was in flight', () => {
    const newText = { kind: 'user_reported', text: 'Original observation plus a correction' };
    expect(afterSuccessfulContextSubmission(newText, submitted)).toBe(newText);
  });
  it('preserves a draft when the type changed but the words did not', () => {
    const differentType = { kind: 'hypothetical', text: 'Original observation' };
    expect(afterSuccessfulContextSubmission(differentType, submitted)).toBe(differentType);
  });
  it('does not treat empty or whitespace-only new drafts as the submitted one', () => {
    const newBlank = { kind: 'user_reported', text: ' ' };
    expect(afterSuccessfulContextSubmission(newBlank, submitted)).toBe(newBlank);
  });
});
