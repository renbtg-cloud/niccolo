// A successful request must not discard an edit the person made while the
// request was in flight. Compare the complete draft, including its kind.
export function afterSuccessfulContextSubmission(current, submitted) {
  if (current.kind === submitted.kind && current.text.trim() === submitted.text) {
    return { ...current, text: '' };
  }
  return current;
}
