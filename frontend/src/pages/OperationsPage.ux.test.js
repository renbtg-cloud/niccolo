import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';

const operations = readFileSync(new URL('./OperationsPage.jsx', import.meta.url), 'utf8');
const situation = readFileSync(new URL('./SituationPage.jsx', import.meta.url), 'utf8');
const app = readFileSync(new URL('../App.jsx', import.meta.url), 'utf8');

describe('primary analysis path', () => {
  it('presents analysis as the primary action while keeping execution plumbing advanced', () => {
    expect(app).toContain('<NavLink to="/">Situation</NavLink><NavLink to="/operations">Analyze</NavLink><NavLink to="/review">Review</NavLink>');
    expect(app).toContain('<summary>Technical</summary>');
    expect(situation).toContain('Analyze this Situation →');
    expect(operations).toContain('<h2>Analyze this Situation</h2>');
    expect(operations).toContain('<summary>Advanced execution</summary>');
    expect(operations.indexOf('<summary>Advanced execution</summary>')).toBeLessThan(operations.indexOf('Reasoning step budget'));
    expect(operations).toContain("run.isPending?'Analyzing…':'Analyze this Situation'");
    expect(operations).toContain('Review reasoning, clarifications and next steps →');
    expect(operations).toContain('<summary>Technical execution details</summary>');
    expect(operations).toContain("operation.state==='FAILED'");
    expect(operations).toContain("operation.state==='CANCELLED'");
    expect(operations).toContain('No answer was published. You can start another analysis.');
  });

  it('reuses operation and analysis IDs after a lost response to avoid duplicate billing', () => {
    expect(operations).toContain('const pendingLaunch=useRef(null)');
    expect(operations).toContain('pendingLaunch.current?.signature!==signature');
    expect(operations).toContain('const {operationId,analysisId}=pendingLaunch.current');
    expect(operations).toContain('onSuccess:()=>{pendingLaunch.current=null;}');
  });
});
