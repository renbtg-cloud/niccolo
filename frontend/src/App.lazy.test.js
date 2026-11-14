import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';

const app = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');

describe('first-user bundle boundary', () => {
  it('keeps the primary Situation, Analyze and Review routes eagerly available', () => {
    for (const name of ['SituationPage', 'OperationsPage', 'ReviewPage']) {
      expect(app).toContain(`import { ${name} } from './pages/${name}.jsx';`);
    }
  });

  it('loads advanced technical surfaces on demand rather than on first visit', () => {
    const technical = [
      'AnalysisPage', 'ReasoningPage', 'FeedbackPage', 'IdentitiesPage',
      'InspectorPage', 'ForensicPage', 'EpistemicPlaygroundPage',
    ];
    for (const name of technical) {
      expect(app).toContain(`lazy(() => import('./pages/${name}.jsx')`);
      expect(app).not.toContain(`import { ${name} } from './pages/${name}.jsx';`);
    }
  });

  it('shows a status during an asynchronous route transition without losing the session reset boundary', () => {
    expect(app).toContain('<Suspense fallback={<p role="status">Loading selected view…</p>}>');
    expect(app).toContain('<Routes key={generation}>');
  });
});
