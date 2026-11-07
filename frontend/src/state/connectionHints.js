export function connectionHints(search) {
  const params = new URLSearchParams(search ?? '');
  return {
    workspace: params.get('workspace') ?? '',
    situation: params.get('situation') ?? '',
    token: '',
  };
}

export function hasPreparedConnectionHints(hints) {
  return Boolean(hints?.workspace && hints?.situation);
}
