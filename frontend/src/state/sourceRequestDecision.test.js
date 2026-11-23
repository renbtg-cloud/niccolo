import { describe, expect, it } from 'vitest';
import { sourceRequestActions, sourceRequestEventRequest } from './sourceRequestDecision.js';

describe('source-request decisions', () => {
  it('keeps active requests decidable and settled requests reopenable', () => {
    expect(sourceRequestActions('PROPOSED')).toEqual(['ACCEPTED', 'DEFERRED', 'DECLINED']);
    expect(sourceRequestActions('REOPENED')).toEqual(['ACCEPTED', 'DEFERRED', 'DECLINED']);
    expect(sourceRequestActions('DEFERRED')).toEqual(['REOPENED']);
    expect(sourceRequestActions('ACCEPTED')).toEqual(['REOPENED']);
  });

  it('normalizes an attributed decision note without implying retrieval', () => {
    expect(sourceRequestEventRequest('DEFERRED', '  I will revisit this tomorrow.  ')).toEqual({
      kind: 'DEFERRED',
      note: 'I will revisit this tomorrow.',
    });
  });

  it('rejects empty, invalid, control-character and oversized notes', () => {
    expect(() => sourceRequestEventRequest('EXECUTE', 'No.')).toThrow(/valid/);
    expect(() => sourceRequestEventRequest('DECLINED', '   ')).toThrow(/Explain/);
    expect(() => sourceRequestEventRequest('DECLINED', 'line\nbreak')).toThrow(/single paragraph/);
    expect(() => sourceRequestEventRequest('DECLINED', 'é'.repeat(2049))).toThrow(/4096/);
  });
});
