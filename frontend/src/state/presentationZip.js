// Minimal STORE-only ZIP writer for the existing bounded presentation package.
// No npm dependency, no server-side archive, no external network publication.
const encoder = new TextEncoder();
const MAX_BYTES = 8 * 1024 * 1024;

function crc32(bytes) {
  let crc = 0xffffffff;
  for (const byte of bytes) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit += 1) {
      crc = (crc >>> 1) ^ ((crc & 1) ? 0xedb88320 : 0);
    }
  }
  return (crc ^ 0xffffffff) >>> 0;
}

function entryPathValid(path) {
  return typeof path === 'string' && path.length > 0
    && !path.startsWith('/') && !path.includes('\\\\')
    && !path.includes(':') && !path.includes(String.fromCharCode(0))
    && path.split('/').every(part => part.length > 0 && part !== '.' && part !== '..');

}

function localHeader(name, length, checksum) {
  const out = new Uint8Array(30 + name.length);
  const view = new DataView(out.buffer);
  view.setUint32(0, 0x04034b50, true);
  view.setUint16(4, 20, true);
  view.setUint16(6, 0x0800, true); // UTF-8 names
  view.setUint32(14, checksum, true);
  view.setUint32(18, length, true);
  view.setUint32(22, length, true);
  view.setUint16(26, name.length, true);
  out.set(name, 30);
  return out;
}

function centralHeader(name, length, checksum, offset) {
  const out = new Uint8Array(46 + name.length);
  const view = new DataView(out.buffer);
  view.setUint32(0, 0x02014b50, true);
  view.setUint16(4, 20, true);
  view.setUint16(6, 20, true);
  view.setUint16(8, 0x0800, true);
  view.setUint32(16, checksum, true);
  view.setUint32(20, length, true);
  view.setUint32(24, length, true);
  view.setUint16(28, name.length, true);
  view.setUint32(42, offset, true);
  out.set(name, 46);
  return out;
}

export function presentationZip(pkg) {
  if (pkg?.format !== 'niccolo.presentation-package' || pkg.published_to_git !== false
      || !pkg.files || typeof pkg.files !== 'object' || Array.isArray(pkg.files)) {
    throw new Error('Invalid portable analysis package.');
  }
  const entries = Object.entries(pkg.files).sort(([a], [b]) => a.localeCompare(b));
  if (entries.length === 0 || entries.length > 65535) {
    throw new Error('Invalid number of export files.');
  }
  const parts = [];
  const directory = [];
  let offset = 0;
  let directorySize = 0;
  let totalPayload = 0;
  for (const [path, value] of entries) {
    if (!entryPathValid(path) || typeof value !== 'string') {
      throw new Error('Invalid export file path or contents.');
    }
    const name = encoder.encode(path);
    const bytes = encoder.encode(value);
    if (name.length > 65535 || bytes.length > MAX_BYTES) {
      throw new Error('Export file exceeds size limits.');
    }
    totalPayload += bytes.length;
    if (totalPayload > MAX_BYTES) throw new Error('Export exceeds size limit.');
    const checksum = crc32(bytes);
    const header = localHeader(name, bytes.length, checksum);
    const central = centralHeader(name, bytes.length, checksum, offset);
    parts.push(header, bytes);
    directory.push(central);
    offset += header.length + bytes.length;
    directorySize += central.length;
  }
  if (offset + directorySize + 22 > 0xffffffff) {
    throw new Error('Export archive is too large.');
  }
  const end = new Uint8Array(22);
  const view = new DataView(end.buffer);
  view.setUint32(0, 0x06054b50, true);
  view.setUint16(8, entries.length, true);
  view.setUint16(10, entries.length, true);
  view.setUint32(12, directorySize, true);
  view.setUint32(16, offset, true);
  return new Blob([...parts, ...directory, end], { type: 'application/zip' });
}
