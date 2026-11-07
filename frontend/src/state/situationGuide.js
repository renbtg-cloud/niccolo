export function situationGuide(snapshot) {
  const focus = snapshot?.focus?.trim?.() ?? '';
  const sourceCount = snapshot?.source_document_ids?.length ?? 0;
  const contextCount = snapshot?.contributions?.length ?? 0;
  return {
    focusSet: Boolean(focus),
    sourceCount,
    contextCount,
    hasGroundingMaterial: sourceCount + contextCount > 0,
    analysisBlocked: false,
  };
}
