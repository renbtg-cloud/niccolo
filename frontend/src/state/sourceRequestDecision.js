import { utf8ByteLength } from './situationText.js';

const allowedKinds = new Set(['ACCEPTED', 'DECLINED', 'DEFERRED', 'REOPENED', 'COMPLETED']);

export function sourceRequestActions(state) {
  if (state === 'PROPOSED' || state === 'REOPENED') return ['ACCEPTED', 'DEFERRED', 'DECLINED'];
  if (state === 'ACCEPTED') return ['COMPLETED', 'REOPENED'];
  return ['REOPENED'];
}

export function sourceRequestEventRequest(kind, note) {
  const clean = note.trim();
  if (!allowedKinds.has(kind)) throw new Error('Choose a valid source-request decision.');
  if (!clean) throw new Error('Explain the decision briefly.');
  if (/\p{Cc}/u.test(clean)) throw new Error('Use a single paragraph without control characters.');
  if (utf8ByteLength(clean) > 4096) throw new Error('The decision note must be at most 4096 UTF-8 bytes.');
  return { kind, note: clean };
}
