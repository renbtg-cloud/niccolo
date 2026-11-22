import { describe, expect, it } from 'vitest';
import { MAX_SOURCE_BYTES, restoreSourceRequest, sourceDescriptor } from './sourceUpload.js';

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
    expect(() => sourceDescriptor({ name: 'image.png', type: 'text/plain', size: 12 })).toThrow(/Unsupported/);
    expect(() => sourceDescriptor({ name: 'chat.csv', type: 'text/plain', size: 12 })).toThrow(/Unsupported/);
    expect(() => sourceDescriptor({ name: 'chat.txt\nX-Bad: value', type: 'text/plain', size: 12 })).toThrow(/file name/);
    expect(() => sourceDescriptor({ name: 'é'.repeat(520) + '.txt', type: 'text/plain', size: 12 })).toThrow(/1024/);
    expect(() => sourceDescriptor({ name: 'chat.txt', type: 'text/plain', size: 0 })).toThrow(/non-empty/);
    expect(() => sourceDescriptor({ name: 'chat.txt', type: 'text/plain', size: MAX_SOURCE_BYTES + 1 })).toThrow(/4 MiB/);
  });
});

describe('original source restoration', () => {
  const source = {
    document_id: 'historical-record',
    byte_length: 20,
    locator: 'evidence://original-name',
    format: 'EML',
  };

  it('uses the existing record identity and format even when the local file was renamed', () => {
    const file = {name: 'copy', size: 20};
    expect(restoreSourceRequest(source, file)).toEqual({
      documentId: source.document_id,
      locator: source.locator,
      contentType: 'message/rfc822',
      body: file,
    });
  });

  it('refuses a file with a different size before upload', () => {
    expect(() => restoreSourceRequest(source, {name: 'copy.eml', size: 21})).toThrow(/size differs/);
  });
});
