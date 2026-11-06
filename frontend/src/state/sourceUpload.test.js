import { describe, expect, it } from 'vitest';
import { MAX_SOURCE_BYTES, sourceDescriptor } from './sourceUpload.js';

describe('source upload boundary', () => {
  it('maps eml and text exports to the backend media types', () => {
    expect(sourceDescriptor({ name: 'mail.eml', type: '', size: 12 })).toEqual({
      locator: 'mail.eml',
      contentType: 'message/rfc822',
    });
    expect(sourceDescriptor({ name: 'chat.txt', type: '', size: 12 })).toEqual({
      locator: 'chat.txt',
      contentType: 'text/plain',
    });
  });

  it('rejects unsupported, empty and oversized sources before upload', () => {
    expect(() => sourceDescriptor({ name: 'image.png', type: 'image/png', size: 12 })).toThrow(/Unsupported/);
    expect(() => sourceDescriptor({ name: 'chat.txt', type: 'text/plain', size: 0 })).toThrow(/non-empty/);
    expect(() => sourceDescriptor({ name: 'chat.txt', type: 'text/plain', size: MAX_SOURCE_BYTES + 1 })).toThrow(/4 MiB/);
  });
});
