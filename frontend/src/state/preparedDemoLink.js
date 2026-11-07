export function preparedDemoUrl(locationLike, config) {
  const origin = locationLike?.origin ?? '';
  const pathname = locationLike?.pathname ?? '/';
  const params = new URLSearchParams();
  params.set('workspace', config.workspace);
  params.set('situation', config.situation);
  return `${origin}${pathname}?${params.toString()}`;
}
