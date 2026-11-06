export const MAX_SOURCE_BYTES = 4 * 1024 * 1024;

export function sourceDescriptor(file) {
  if (!file || typeof file.name !== 'string' || !file.name.trim()) {
    throw new Error('Choose an email (.eml) or WhatsApp/text export (.txt).');
  }
  if (!Number.isFinite(file.size) || file.size <= 0 || file.size > MAX_SOURCE_BYTES) {
    throw new Error('Source must be non-empty and at most 4 MiB.');
  }
  const name = file.name.trim();
  const lower = name.toLowerCase();
  let contentType = null;
  if (file.type === 'message/rfc822' || lower.endsWith('.eml')) {
    contentType = 'message/rfc822';
  } else if (file.type === 'text/plain' || lower.endsWith('.txt')) {
    contentType = 'text/plain';
  }
  if (!contentType) {
    throw new Error('Unsupported source type. Use .eml or .txt.');
  }
  return { locator: name, contentType };
}
