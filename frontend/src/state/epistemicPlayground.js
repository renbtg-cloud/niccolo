// Fictional, reproducible examples. Never substitute these IDs for real
// documentary evidence in a grounded evaluation or analysis.
const EVIDENCE = Object.freeze([
  '00000000-0000-4000-8000-000000000001',
  '00000000-0000-4000-8000-000000000002',
  '00000000-0000-4000-8000-000000000003',
]);
const ORIGINS = Object.freeze([
  '00000000-0000-4000-8000-000000000101',
  '00000000-0000-4000-8000-000000000102',
  '00000000-0000-4000-8000-000000000103',
]);

export const initialPlaygroundSettings = Object.freeze({
  supportMultiplier: '100',
  counterMultiplier: '100',
  perOriginCap: '100',
  humanConfidence: '90',
  firstStrength: '80',
  copyStrength: '70',
  counterStrength: '60',
  sharedOrigin: true,
});

export function boundedInteger(value, min, max, label) {
  if (typeof value !== 'string' && typeof value !== 'number') {
    throw new Error(label + ' must be a number');
  }
  if (String(value).trim() === '') {
    throw new Error(label + ' is required');
  }
  const n = Number(value);
  if (!Number.isInteger(n) || n < min || n > max) {
    throw new Error(label + ' must be an integer from ' + min + ' to ' + max);
  }
  return n;
}

// Backend is the sole owner of the arithmetic and of stored policy resolution.
// No client-supplied input in this preview is asserted to exist in the DB.
export function comparisonRequest(settings, situationId) {
  const first = boundedInteger(settings.firstStrength, 0, 100, 'First observation strength');
  const copy = boundedInteger(settings.copyStrength, 0, 100, 'Second observation strength');
  const counter = boundedInteger(settings.counterStrength, 0, 100, 'Counterevidence strength');
  const confidence = String(settings.humanConfidence ?? '').trim();
  return {
    situation_id: situationId,
    trial_policy: {
      support_multiplier_pct: boundedInteger(settings.supportMultiplier, 0, 200, 'Support multiplier'),
      counter_multiplier_pct: boundedInteger(settings.counterMultiplier, 0, 200, 'Counter multiplier'),
      per_origin_cap: boundedInteger(settings.perOriginCap, 0, 100, 'Per-origin cap'),
    },
    human_declared_confidence_percent: confidence === ''
      ? null
      : boundedInteger(confidence, 0, 100, 'Human confidence'),
    observations: [
      {
        evidence_id: EVIDENCE[0],
        documented_origin_id: ORIGINS[0],
        stance: 'SUPPORTS',
        declared_strength: first,
        rationale: 'Fictional report A',
      },
      {
        evidence_id: EVIDENCE[1],
        documented_origin_id: settings.sharedOrigin ? ORIGINS[0] : ORIGINS[1],
        stance: 'SUPPORTS',
        declared_strength: copy,
        rationale: settings.sharedOrigin ? 'Fictional copy of report A' : 'Fictional report B',
      },
      {
        evidence_id: EVIDENCE[2],
        documented_origin_id: ORIGINS[2],
        stance: 'CONTRADICTS',
        declared_strength: counter,
        rationale: 'Fictional contrary report',
      },
    ],
  };
}
