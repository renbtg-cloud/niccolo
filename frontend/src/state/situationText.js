const encoder = new TextEncoder();

export function situationTextError(input) {
  const text = input.trim();
  if (!text) return null;
  if (/\p{Cc}/u.test(text)) return 'Use a single paragraph without line breaks or control characters.';
  if (encoder.encode(text).length > 4096) return 'Text must be at most 4096 UTF-8 bytes.';
  return null;
}
