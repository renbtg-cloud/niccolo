const encoder = new TextEncoder();

export const utf8ByteLength = text => encoder.encode(text).length;

export function situationTextError(input, maxBytes = 4096) {
  const text = input.trim();
  if (!text) return null;
  if (/\p{Cc}/u.test(text)) return 'Use a single paragraph without line breaks or control characters.';
  if (utf8ByteLength(text) > maxBytes) return `Text must be at most ${maxBytes} UTF-8 bytes.`;
  return null;
}
