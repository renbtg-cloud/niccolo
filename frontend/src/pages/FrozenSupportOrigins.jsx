function routeKey(route) {
  return `${route.argument_id}:${route.grounding_set_id}`;
}

export function FrozenSupportOrigins({basis}) {
  const origins = basis?.shared_artifact_origins ?? [];
  const truncated = basis?.shared_artifact_origins_truncated;
  if (origins.length === 0 && !truncated) return null;

  const routes = new Map((basis.routes ?? []).map((route) => [routeKey(route), route]));
  return <section>
    <h4>Shared origins in this analysis</h4>
    {truncated && <p>The analysis received a limited set of recorded derivations.</p>}
    <ul>{origins.map((origin) => {
      const keys = new Set(origin.derivations.flatMap((derivation) => derivation.routes.map(routeKey)));
      const conclusion = [...keys].map((key) => routes.get(key)?.conclusion_text).find(Boolean);
      return <li key={`${origin.conclusion_object_id}:${origin.source_representation_id}`}>
        {conclusion && <p><strong>{conclusion}</strong></p>}
        <p>{keys.size} supporting routes trace to {origin.source_artifact_label} · {origin.source_scope_locator}.</p>
      </li>;
    })}</ul>
  </section>;
}

export function PublishedSupportOrigins({basis, citations}) {
  if (!basis?.shared_artifact_origins?.length || !citations?.length) return null;

  const routesByClaim = new Map();
  for (const citation of citations) {
    if (!routesByClaim.has(citation.claim_id)) routesByClaim.set(citation.claim_id, new Set());
    routesByClaim.get(citation.claim_id).add(routeKey(citation));
  }
  const findings = [];
  for (const origin of basis.shared_artifact_origins) {
    const originRoutes = new Set(origin.derivations.flatMap((derivation) => derivation.routes.map(routeKey)));
    const counts = [...routesByClaim.values()].map((routes) =>
      [...routes].filter((route) => originRoutes.has(route)).length,
    );
    if (counts.some((count) => count >= 2)) findings.push(origin);
  }
  if (findings.length === 0) return null;

  return <section>
    <h4>Shared origins in the published answer</h4>
    <ul>{findings.map((origin) => <li key={`${origin.conclusion_object_id}:${origin.source_representation_id}`}>
      Multiple routes cited for the same claim trace to {origin.source_artifact_label} · {origin.source_scope_locator}.
    </li>)}</ul>
  </section>;
}
