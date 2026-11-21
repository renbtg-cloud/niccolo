// The investigator chooses which questions to compare. This projection does
// not order candidates by quality, infer a missing judgment or grant authority.
export const inquiryCriteria = Object.freeze([
  ['RELEVANCE', 'Relevance'],
  ['DISCRIMINATORY_POWER', 'Discriminatory power'],
  ['ACQUISITION_COST', 'Acquisition cost'],
  ['ACCESSIBILITY', 'Accessibility'],
  ['CONTAMINATION_RISK', 'Contamination risk'],
  ['URGENCY', 'Urgency'],
  ['AUTHORIZATION_STATUS', 'Reported authorization (not permission)'],
  ['INVESTIGATION_COVERAGE', 'Investigation coverage'],
]);

export function comparisonQuestions(snapshot, selectedIds) {
  const questions = new Map((snapshot.question_inquiries ?? []).map(q => [q.question_object_id, q]));
  const nodes = new Map((snapshot.nodes ?? []).filter(n => n.kind === 'QUESTION').map(n => [n.object_id,n]));
  const assessments = new Map((snapshot.assessments ?? []).map(a => [a.event_id,a]));
  const superseded = new Set((snapshot.assessments ?? []).map(a => a.input.revises_event_id).filter(Boolean));
  return [...new Set(selectedIds)].filter(id => questions.has(id) && nodes.has(id)).slice(0,3).map(id => {
    const q = questions.get(id);
    const criteria = inquiryCriteria.map(([dimension,label]) => {
      const projected = dimension === 'RELEVANCE' ? q.current_relevance_events : dimension === 'DISCRIMINATORY_POWER' ? q.current_discriminatory_power_events : q.selection_criteria?.find(c => c.dimension === dimension)?.current_events;
      const references = [...new Set(projected ?? [])];
      const current = references.map(eventId => {
        const event = assessments.get(eventId);
        // Fail closed on inconsistent projections: never display another
        // question's or another dimension's assessment in this cell.
        return event && !superseded.has(eventId) && event.input.dimension === dimension && event.input.target.kind === 'OBJECT' && event.input.target.object_id === id ? event : null;
      });
      return {dimension,label,judgments:current.filter(Boolean),projectionIncomplete:projected == null || current.some(event=>!event)};
    });
    return {question:nodes.get(id),criteria,links:{gaps:q.addressed_gaps ?? [],hypotheses:q.tested_hypotheses ?? [],presuppositions:q.presuppositions ?? []}};
  });
}
