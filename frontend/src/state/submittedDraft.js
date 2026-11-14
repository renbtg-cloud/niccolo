// Do not erase new text typed while an earlier human action was in flight.
// Compare normalized user text because API writes its trimmed form.
export function clearOnlySubmittedText(current, submitted) {
  return current.trim() === (submitted ?? '') ? '' : current;
}
