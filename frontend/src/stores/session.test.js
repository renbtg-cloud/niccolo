import { afterEach, expect, it } from 'vitest';
import { isCurrentSession, useSessionStore } from './session.js';

afterEach(() => useSessionStore.getState().disconnect());

it('rejects late completions after logout and after a same-Situation reconnect', () => {
  const credentials = {workspace:'w', situation:'s', token:'fixture'};
  useSessionStore.getState().connect(credentials);
  const original = useSessionStore.getState().generation;
  expect(isCurrentSession(original)).toBe(true);
  useSessionStore.getState().disconnect();
  expect(isCurrentSession(original)).toBe(false);
  useSessionStore.getState().connect(credentials);
  expect(isCurrentSession(original)).toBe(false);
  expect(isCurrentSession(useSessionStore.getState().generation)).toBe(true);
});


it('holds an investigation draft only in its active session', () => {
  const credentials = {workspace:'w', situation:'s', token:'fixture'};
  useSessionStore.getState().connect(credentials);
  const generation = useSessionStore.getState().generation;
  useSessionStore.getState().stageInvestigationDraft('Look for the original', generation);
  expect(useSessionStore.getState().investigationDraft).toBe('Look for the original');
  useSessionStore.getState().connect(credentials);
  useSessionStore.getState().stageInvestigationDraft('Late old draft', generation);
  expect(useSessionStore.getState().investigationDraft).toBe('');
});
