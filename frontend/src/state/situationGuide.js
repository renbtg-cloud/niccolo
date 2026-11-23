export function situationGuide(snapshot, artifactSummary = {}) {
  const focus = snapshot?.focus?.trim?.() ?? '';
  const contextCount = snapshot?.contributions?.length ?? 0;
  const artifactCount = artifactSummary.artifactCount ?? 0;
  const representationCount = artifactSummary.representationCount ?? 0;
  const relationshipCount = artifactSummary.relationshipCount ?? 0;
  return {
    focusSet: Boolean(focus),
    artifactCount,
    representationCount,
    relationshipCount,
    contextCount,
    hasGroundingMaterial: artifactCount + representationCount + contextCount > 0,
    analysisBlocked: false,
  };
}
