import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';

const source = readFileSync(new URL('./ConnectPanel.jsx', import.meta.url), 'utf8');

describe('connection boundary', () => {
  it('clears analysis and active operation when credentials are disconnected', () => {
    expect(source).toContain("setAnalysis('')");
    expect(source).toContain("setActiveOperation('')");
    expect(source).toContain('disconnectClean');
  });
  it('discards authenticated query data before switching or ending sessions', () => {
    expect(source).toContain('const queryClient=useQueryClient()');
    expect(source).toContain('queryClient.clear();clearUi();');
    expect(source).toContain('clearSessionView();connect(credentials);');
    expect(source).toContain('const disconnectClean=()=>{disconnect();clearSessionView();}');
    expect(source).toContain('handleSubmit(connectClean)');
  });

  it('validates workspace access before marking a disconnected form as connected', () => {
    expect(source).toContain('await validateConnection(credentials)');
    expect(source).toContain("setConnectionError(connectionFailureMessage(error))");
    expect(source).toContain("checking?'Checking…':'Connect'");
    expect(source).toContain('disabled={checking}');
  });

  it('focuses only the secret field for a prepared session without persisting it', () => {
    expect(source).toContain('autoFocus={prepared}');
    expect(source).toContain('type="password"');
    expect(source).toContain('autoComplete="off"');
  });

  it('never reads a bearer token from URL hints', () => {
    expect(source).toContain('connectionHints(window.location.search)');
    expect(source).toContain('Bearer tokens are never read from the URL or persisted.');
  });
});
