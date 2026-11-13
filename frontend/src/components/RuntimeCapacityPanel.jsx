function pressureLabel(value) {
  return ({ NORMAL: 'normal', CONSTRAINED: 'constrained', CRITICAL: 'critical', UNKNOWN: 'unknown' })[value] ?? 'unknown';
}

function reasonLabel(value) {
  return ({
    CPU_CAPACITY: 'CPU capacity',
    CRITICAL_MEMORY_PRESSURE: 'critical memory pressure',
    HIGH_MEMORY_PRESSURE: 'high memory pressure',
    MEMORY_PRESSURE_UNKNOWN: 'memory pressure unavailable',
    OPERATOR_DELEGATED_CONTROL: 'concurrency delegated to the operator',
  })[value] ?? value.replaceAll('_',' ').toLowerCase();
}

export function RuntimeCapacityPanel({ diagnostic, pending, error }) {
  if (pending) return <details className="runtime-capacity"><summary>Runtime capacity</summary><p>Checking local capacity…</p></details>;
  if (error) return <details className="runtime-capacity"><summary>Runtime capacity unavailable</summary><p><small>Niccolò could not observe the current runtime envelope. Missing telemetry does not authorize skipping reasoning work.</small></p></details>;
  if (!diagnostic) return null;
  const pressure=pressureLabel(diagnostic.pressure);
  const memoryPercent=Number.isInteger(diagnostic.memory_pressure_basis_points)
    ? (diagnostic.memory_pressure_basis_points/100).toFixed(1)
    : null;
  const reasons=(diagnostic.advice_reasons??[]).map(reasonLabel);
  return <details className={`runtime-capacity pressure-${pressure}`} open={diagnostic.pressure==='CRITICAL'}>
    <summary>Runtime capacity: {pressure}</summary>
    <p>Current policy recommends up to <strong>{diagnostic.recommended_parallel_operations}</strong> concurrent operation(s) from an observed ceiling of <strong>{diagnostic.observed_parallel_ceiling}</strong>.</p>
    {memoryPercent!==null&&<p>Observed memory pressure: <strong>{memoryPercent}%</strong>.</p>}
    {diagnostic.defer_nonessential_new_work&&<p><strong>Critical pressure:</strong> Niccolò may defer explicitly nonessential reasoning passes while preserving synthesis, adversarial challenge and human control.</p>}
    {reasons.length>0&&<p><small>Operational reasons: {reasons.join(' · ')}.</small></p>}
    <small>This is operational capacity advice only. It is not confidence, epistemic quality, completeness or truth, and Niccolò does not mutate host resource limits.</small>
  </details>;
}
