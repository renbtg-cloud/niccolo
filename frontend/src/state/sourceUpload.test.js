import { describe, expect, it } from 'vitest';
import {
  MAX_SOURCE_BATCH_BYTES,
  MAX_SOURCE_BATCH_FILES,
  MAX_SOURCE_BYTES,
  sourceBatchDescriptors,
  sourceDescriptor,
} from './sourceUpload.js';

describe('source upload boundary', () => {
  it('maps eml and WhatsApp txt exports to the backend media types', () => {
    expect(sourceDescriptor({ name: 'mail.eml', type: '', size: 12 })).toEqual({
      locator: 'mail.eml',
      contentType: 'message/rfc822',
    });
    expect(sourceDescriptor({ name: 'chat.txt', type: '', size: 12 })).toEqual({
      locator: 'chat.txt',
      contentType: 'text/plain',
    });
  });

  it('does not describe arbitrary plain text as a supported source contract', () => {
    expect(() => sourceDescriptor(null)).toThrow('Choose an email (.eml) or WhatsApp export (.txt).');
    expect(() => sourceDescriptor({ name: 'notes.md', type: 'text/plain', size: 12 }))
      .toThrow('Use an .eml email or .txt WhatsApp export.');
  });

  it('rejects unsupported, empty and oversized sources before upload', () => {
    expect(() => sourceDescriptor({ name: 'image.png', type: 'image/png', size: 12 })).toThrow(/Unsupported/);
    expect(() => sourceDescriptor({ name: 'image.png', type: 'text/plain', size: 12 })).toThrow(/Unsupported/);
    expect(() => sourceDescriptor({ name: 'chat.csv', type: 'text/plain', size: 12 })).toThrow(/Unsupported/);
    expect(() => sourceDescriptor({ name: 'chat.txt\nX-Bad: value', type: 'text/plain', size: 12 })).toThrow(/file name/);
    expect(() => sourceDescriptor({ name: 'chat\u0085name.txt', type: 'text/plain', size: 12 })).toThrow(/file name/);
    expect(() => sourceDescriptor({ name: 'é'.repeat(520) + '.txt', type: 'text/plain', size: 12 })).toThrow(/1024/);
    expect(() => sourceDescriptor({ name: 'chat.txt', type: 'text/plain', size: 0 })).toThrow(/non-empty/);
    expect(() => sourceDescriptor({ name: 'chat.txt', type: 'text/plain', size: MAX_SOURCE_BYTES + 1 })).toThrow(/4 MiB/);
  });
});


describe('sourceBatchDescriptors', () => {
  const file = (name, size = 128) => ({ name, size });

  it('validates the entire batch before upload and preserves descriptor order', () => {
    expect(sourceBatchDescriptors([file('a.txt'), file('b.eml')])).toEqual([
      { locator: 'a.txt', contentType: 'text/plain' },
      { locator: 'b.eml', contentType: 'message/rfc822' },
    ]);
  });

  it('bounds file count and aggregate selected bytes', () => {
    expect(() => sourceBatchDescriptors([])).toThrow(/at least one/);
    expect(() =>
      sourceBatchDescriptors(Array.from({ length: MAX_SOURCE_BATCH_FILES + 1 }, (_, i) => file(`${i}.txt`)))
    ).toThrow(/at most/);
    const chunk = Math.floor(MAX_SOURCE_BATCH_BYTES / 17) + 1;
    expect(chunk).toBeLessThanOrEqual(MAX_SOURCE_BYTES);
    expect(() =>
      sourceBatchDescriptors(Array.from({ length: 17 }, (_, i) => file(`${i}.txt`, chunk)))
    ).toThrow(/64 MiB/);
  });

  it('rejects any invalid member before a caller starts network work', () => {
    expect(() => sourceBatchDescriptors([file('good.txt'), file('bad.pdf')])).toThrow(/Unsupported source type/);
  });
});
