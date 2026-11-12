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
    expect(source).toContain("checking?'Connecting…':'Connect'");
    expect(source).toContain('disabled={checking}');
  });

  it('starts in connecting state when a valid handoff fragment is already present', () => {
    expect(source).toContain('useState(()=>hasLocalHandoffFragment(window.location))');
    expect(source).toContain('disabled={checking}');
    expect(source).toContain("checking?'Connecting…':'Connect'");
  });

  it('removes a one-shot local capability before redeeming it and keeps bearer fallback explicit', () => {
    expect(source).toContain('connectionHints(window.location.search)');
    expect(source).toContain('takeLocalHandoffFromFragment(window.location,window.history)');
    expect(source).toContain('await redeemLocalHandoff(code)');
    expect(source.indexOf('takeLocalHandoffFromFragment(window.location,window.history)')).toBeLessThan(source.indexOf('await redeemLocalHandoff(code)'));
    expect(source).toContain('connectionSchema.safeParse({...getValues(),token})');
    expect(source).toContain('.niccolo-local/token.txt');
    expect(source).toContain('Bearer tokens are never placed in the URL or persisted.');
  });
});
