export const relationKinds = Object.freeze({
  SEMANTIC: ['EQUIVALENT_TO', 'MORE_SPECIFIC_THAN', 'ENTAILS', 'CONTRADICTS', 'PARTIAL_OVERLAP'],
  GENEALOGY: ['REFINES', 'SUPERSEDES', 'RETRACTS', 'CONSOLIDATES', 'SEPARATES', 'REINTERPRETS'],
  CONTEXTUAL: ['RELEVANT_TO', 'APPLICABLE_UNDER'],
  CAUSAL: ['CAUSES', 'CONTRIBUTES_TO', 'ENABLES', 'PREVENTS', 'INTENSIFIES', 'ATTENUATES'],
});


export const relationSemantics = Object.freeze({
  EQUIVALENT_TO: { left: 'Object A', right: 'Object B', description: 'The two objects express equivalent meaning in this context.' },
  MORE_SPECIFIC_THAN: { left: 'More specific object', right: 'Broader object', description: 'The left object narrows the meaning of the right object.' },
  ENTAILS: { left: 'Entailing object', right: 'Entailed object', description: 'Accepting the left object commits the model to the right object under the stated rationale.' },
  CONTRADICTS: { left: 'Object A', right: 'Object B', description: 'The two objects cannot both hold as stated in the same relevant context.' },
  PARTIAL_OVERLAP: { left: 'Object A', right: 'Object B', description: 'The objects overlap semantically without being equivalent.' },
  REFINES: { left: 'Refined object', right: 'Prior object', description: 'The left object adds precision to the right object without necessarily replacing it.' },
  SUPERSEDES: { left: 'Replacement object', right: 'Superseded object', description: 'The left object replaces the right object for subsequent reasoning.' },
  RETRACTS: { left: 'Retraction object', right: 'Retracted object', description: 'The left object explicitly withdraws the right object from active use.' },
  CONSOLIDATES: { left: 'Consolidated object', right: 'Source object', description: 'The left object incorporates the right object into a consolidated representation.' },
  SEPARATES: { left: 'Separated object', right: 'Prior combined object', description: 'The left object separates a distinction that the right object previously combined.' },
  REINTERPRETS: { left: 'New interpretation', right: 'Prior object', description: 'The left object provides a revised reading of the right object.' },
  RELEVANT_TO: { left: 'Relevant object', right: 'Context object', description: 'The left object is relevant to the right object in the stated context.' },
  APPLICABLE_UNDER: { left: 'Applicable object', right: 'Condition object', description: 'The left object applies under the condition represented by the right object.' },
  CAUSES: { left: 'Cause', right: 'Effect', description: 'The left object is asserted as a cause of the right object.' },
  CONTRIBUTES_TO: { left: 'Contributor', right: 'Outcome', description: 'The left object contributes causally to the right object without claiming sole causation.' },
  ENABLES: { left: 'Enabler', right: 'Enabled outcome', description: 'The left object enables the right object.' },
  PREVENTS: { left: 'Preventer', right: 'Prevented outcome', description: 'The left object prevents the right object.' },
  INTENSIFIES: { left: 'Intensifier', right: 'Affected outcome', description: 'The left object increases the intensity or likelihood represented by the right object.' },
  ATTENUATES: { left: 'Attenuator', right: 'Affected outcome', description: 'The left object reduces the intensity or likelihood represented by the right object.' },
});

export const nodeKinds = Object.freeze(['PROPOSITION', 'HYPOTHESIS', 'ASSUMPTION', 'INFORMATION_GAP', 'FINDING']);
export const argumentModes = Object.freeze(['DEDUCTIVE', 'INDUCTIVE', 'ABDUCTIVE', 'CAUSAL', 'ANALOGICAL', 'POLICY', 'OTHER']);
export const groundingEffects = Object.freeze(['SUPPORTS', 'WEAKENS', 'CHALLENGES', 'CONDITIONS']);
export const groundingAdequacies = Object.freeze(['UNSPECIFIED', 'PARTIAL', 'SUFFICIENT', 'NECESSARY_CONDITION']);
export const reassessmentDispositions = Object.freeze(['RETAINED', 'REVISION_REQUIRED', 'DEFERRED']);
export const applicabilityAssessments = Object.freeze(['UNASSESSED', 'SATISFIED', 'NOT_SATISFIED', 'CONTESTED', 'UNKNOWN']);
export const assessmentResults = Object.freeze({
  CREDIBILITY: ['CREDIBLE', 'QUESTIONABLE', 'NOT_CREDIBLE', 'UNKNOWN', 'CONTESTED'],
  RELEVANCE: ['RELEVANT', 'LIMITED_RELEVANCE', 'IRRELEVANT', 'UNKNOWN', 'CONTESTED'],
  RELIABILITY: ['RELIABLE', 'LIMITED_RELIABILITY', 'UNRELIABLE', 'UNKNOWN', 'CONTESTED'],
  INDEPENDENCE: ['INDEPENDENCE_SUPPORTED', 'DEPENDENCE_INDICATED', 'DEPENDENT', 'UNKNOWN', 'CONTESTED'],
  DISCRIMINATORY_POWER: ['HIGH_POWER', 'LIMITED_POWER', 'NO_POWER', 'UNKNOWN', 'CONTESTED'],
});
export const assessmentConfidences = Object.freeze(['LOW', 'MODERATE', 'HIGH', 'UNKNOWN']);
export function assessmentKey(target) {
  return target.kind === 'ROUTE' ? `ROUTE:${target.argument_id}:${target.grounding_set_id}` : `${target.kind}:${target.object_id ?? target.evidence_id ?? target.relation_id}`;
}
export function assessmentRequest(draft, target, grounds) {
  if (!target || !assessmentResults[draft.dimension]?.includes(draft.result) || !assessmentConfidences.includes(draft.confidence)) throw new Error('Choose a target, dimension-specific result and confidence.');
  if (grounds.length > 64 || new Set(grounds.map(assessmentKey)).size !== grounds.length) throw new Error('Choose at most 64 distinct grounds.');
  return { target, dimension: draft.dimension, result: draft.result, confidence: draft.confidence, criterion: requireText(draft.criterion, 'Criterion', 2048), context: requireText(draft.context, 'Context', 2048), rationale: requireText(draft.rationale, 'Rationale'), grounds, revises_event_id: draft.revises_event_id ?? null };
}

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

export function groundingRequest(effect, adequacy, rationale, selectedNodes, roles, selectedEvidence = [], memberRationales = {}) {
  if (!groundingEffects.includes(effect) || !groundingAdequacies.includes(adequacy)) throw new Error('Choose grounding semantics.');
  if (!selectedNodes.length && !selectedEvidence.length) throw new Error('Choose at least one joint ground.');
  return {
    effect,
    adequacy,
    rationale: requireText(rationale, 'Rationale'),
    members: [
      ...selectedNodes.map((node) => ({
        member_id: crypto.randomUUID(),
        role: roles[node.object_id] ?? (node.kind === 'ASSUMPTION' ? 'ASSUMPTION' : 'PREMISE'),
        ground_object_id: node.object_id,
        evidence_id: null,
        rationale: requireText(memberRationales[`object:${node.object_id}`], 'Ground rationale'),
      })),
      ...selectedEvidence.map((item) => ({
        member_id: crypto.randomUUID(),
        role: 'EVIDENCE',
        ground_object_id: null,
        evidence_id: item.evidence_id,
        rationale: requireText(memberRationales[`evidence:${item.evidence_id}`], 'Evidence rationale'),
      })),
    ],
  };
}

export function reassessmentReviewRequest(groundingSetId, disposition, note) {
  if (!reassessmentDispositions.includes(disposition)) throw new Error('Choose a reassessment disposition.');
  return {
    grounding_set_id: groundingSetId || null,
    disposition,
    note: requireText(note, 'Review note'),
  };
}

export function applicabilityRequest(conditionObjectId, assessment, rationale) {
  if (!conditionObjectId || !applicabilityAssessments.includes(assessment)) throw new Error('Choose a condition and applicability assessment.');
  return { condition_object_id: conditionObjectId, assessment, rationale: requireText(rationale, 'Assessment rationale') };
}
