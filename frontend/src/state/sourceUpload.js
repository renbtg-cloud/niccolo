export const MAX_SOURCE_BYTES = 4 * 1024 * 1024;
export const MAX_SOURCE_BATCH_FILES = 32;
export const MAX_SOURCE_BATCH_BYTES = 64 * 1024 * 1024;

export function sourceDescriptor(file) {
  if (!file || typeof file.name !== 'string' || !file.name.trim()) {
    throw new Error('Choose an email (.eml) or WhatsApp export (.txt).');
  }
  if (!Number.isFinite(file.size) || file.size <= 0 || file.size > MAX_SOURCE_BYTES) {
    throw new Error('Source must be non-empty and at most 4 MiB.');
  }
  const name = file.name.trim();
  // Match the server's 1024-byte UTF-8 locator limit and control-character
  // rejection before transferring a potentially multi-megabyte source.
  if (new TextEncoder().encode(name).length > 1024 || /\p{Cc}/u.test(name)) {
    throw new Error('Source file name is invalid or exceeds 1024 UTF-8 bytes.');
  }
  const lower = name.toLowerCase();
  // The extension identifies which parser to invoke. Browser-provided MIME
  // types are advisory, and must not allow image.png to pass as a text export.
  let contentType;
  if (lower.endsWith('.eml')) {
    contentType = 'message/rfc822';
  } else if (lower.endsWith('.txt')) {
    contentType = 'text/plain';
  } else {
    throw new Error('Unsupported source type. Use an .eml email or .txt WhatsApp export.');
  }
  return { locator: name, contentType };
}


export function sourceBatchDescriptors(files) {
  const selected = Array.from(files ?? []);
  if (selected.length === 0) throw new Error('Choose at least one source file.');
  if (selected.length > MAX_SOURCE_BATCH_FILES) {
    throw new Error(`Choose at most ${MAX_SOURCE_BATCH_FILES} source files at once.`);
  }
  const descriptors = selected.map(sourceDescriptor);
  const totalBytes = selected.reduce((sum, file) => sum + file.size, 0);
  if (!Number.isSafeInteger(totalBytes) || totalBytes > MAX_SOURCE_BATCH_BYTES) {
    throw new Error('Selected source batch must be at most 64 MiB.');
  }
  return descriptors;
}
