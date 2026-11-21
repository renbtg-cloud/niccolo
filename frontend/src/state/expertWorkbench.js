export const relationKinds = Object.freeze({
  SEMANTIC: ['EQUIVALENT_TO', 'MORE_SPECIFIC_THAN', 'ENTAILS', 'CONTRADICTS', 'PARTIAL_OVERLAP'],
  GENEALOGY: ['REFINES', 'SUPERSEDES', 'RETRACTS', 'CONSOLIDATES', 'SEPARATES', 'REINTERPRETS'],
  CONTEXTUAL: ['RELEVANT_TO', 'APPLICABLE_UNDER'],
  CAUSAL: ['CAUSES', 'CONTRIBUTES_TO', 'ENABLES', 'PREVENTS', 'INTENSIFIES', 'ATTENUATES'],
});

export const nodeKinds = Object.freeze(['HYPOTHESIS', 'ASSUMPTION', 'INFORMATION_GAP', 'FINDING']);
export const argumentModes = Object.freeze(['DEDUCTIVE', 'INDUCTIVE', 'ABDUCTIVE', 'CAUSAL', 'ANALOGICAL', 'POLICY', 'OTHER']);
export const groundingEffects = Object.freeze(['SUPPORTS', 'WEAKENS', 'CHALLENGES', 'CONDITIONS']);
export const groundingAdequacies = Object.freeze(['UNSPECIFIED', 'PARTIAL', 'SUFFICIENT', 'NECESSARY_CONDITION']);

export function requireText(value, label, max = 4096) {
  const text = String(value ?? '').trim();
  if (!text || text.length > max) throw new Error(`${label} must be 1–${max} characters.`);
  return text;
}

export function nodeRequest(kind, text, rationale) {
  if (!nodeKinds.includes(kind)) throw new Error('Choose a supported epistemic abstraction.');
  return { kind, text: requireText(text, 'Statement', 8192), rationale: requireText(rationale, 'Rationale') };
}

export function relationRequest(family, kind, leftObjectId, rightObjectId, rationale) {
  if (!relationKinds[family]?.includes(kind)) throw new Error('Relation kind does not belong to the selected family.');
  if (!leftObjectId || !rightObjectId || leftObjectId === rightObjectId) throw new Error('Choose two distinct epistemic objects.');
  return {
    family,
    kind,
    left_object_id: leftObjectId,
    right_object_id: rightObjectId,
    rationale: requireText(rationale, 'Rationale'),
  };
}

export function argumentRequest(conclusionObjectId, mode, rationale) {
  if (!conclusionObjectId || !argumentModes.includes(mode)) throw new Error('Choose a conclusion and argument mode.');
  return { conclusion_object_id: conclusionObjectId, mode, rationale: requireText(rationale, 'Rationale') };
}

export function groundingRequest(effect, adequacy, rationale, selectedNodes, roles) {
  if (!groundingEffects.includes(effect) || !groundingAdequacies.includes(adequacy)) throw new Error('Choose grounding semantics.');
  if (!selectedNodes.length) throw new Error('Choose at least one joint ground.');
  return {
    effect,
    adequacy,
    rationale: requireText(rationale, 'Rationale'),
    members: selectedNodes.map((node) => ({
      member_id: crypto.randomUUID(),
      role: roles[node.object_id] ?? (node.kind === 'ASSUMPTION' ? 'ASSUMPTION' : 'PREMISE'),
      ground_object_id: node.object_id,
      evidence_id: null,
      rationale: `Explicit ${node.kind.toLowerCase()} selected by the investigator as a grounding member.`,
    })),
  };
}
