import { describe, expect, it } from 'vitest';
import { presentationZip } from './presentationZip.js';

function pkg(files) {
  return { format: 'niccolo.presentation-package', published_to_git: false, files };
}

describe('portable analysis ZIP', () => {
  it('writes readable uncompressed ZIP entries with correct CRC and directory', async () => {
    const archive = presentationZip(pkg({ 'data/numbers.txt': '123456789', 'README.md': 'Private analysis' }));
    expect(archive.type).toBe('application/zip');
    const bytes = new Uint8Array(await archive.arrayBuffer());
    const view = new DataView(bytes.buffer);
    expect(view.getUint32(0, true)).toBe(0x04034b50);
    const firstNameLength = view.getUint16(26, true);
    const firstName = new TextDecoder().decode(bytes.subarray(30, 30 + firstNameLength));
    expect(firstName).toBe('data/numbers.txt');
    expect(view.getUint32(14, true)).toBe(0xcbf43926);
    const firstContent = new TextDecoder().decode(bytes.subarray(30 + firstNameLength, 30 + firstNameLength + 9));
    expect(firstContent).toBe('123456789');
    const endOffset = bytes.length - 22;
    expect(view.getUint32(endOffset, true)).toBe(0x06054b50);
    expect(view.getUint16(endOffset + 10, true)).toBe(2);
    const directoryOffset = view.getUint32(endOffset + 16, true);
    expect(view.getUint32(directoryOffset, true)).toBe(0x02014b50);
  });

  it('rejects traversal, malformed payload and oversized exports', () => {
    expect(() => presentationZip(pkg({ '../secret': 'bad' }))).toThrow();
    expect(() => presentationZip(pkg({ '/absolute': 'bad' }))).toThrow();
    expect(() => presentationZip(pkg({ 'C:/absolute': 'bad' }))).toThrow();
    expect(() => presentationZip(pkg({ ['bad' + String.fromCharCode(92) + 'name']: 'bad' }))).toThrow();
    expect(() => presentationZip(pkg({ ['bad' + String.fromCharCode(0) + 'name']: 'bad' }))).toThrow();
    expect(() => presentationZip(pkg({ 'README.md': 'x'.repeat(8 * 1024 * 1024 + 1) }))).toThrow();
    expect(() => presentationZip({ ...pkg({ 'README.md': 'ok' }), published_to_git: true })).toThrow();
    expect(() => presentationZip(pkg({}))).toThrow();
  });
});
