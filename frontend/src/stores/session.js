import { create } from 'zustand';
export const useSessionStore = create((set) => ({
  workspace: '', situation: '', token: '', connected: false, generation: 0, investigationDraft: '',
  // A monotonically increasing session generation scopes local component drafts
  // even when credentials change for the same workspace and Situation.
  connect: ({ workspace, situation, token }) => set((state) => ({
    workspace, situation, token, connected: true, investigationDraft: '', generation: state.generation + 1,
  })),
  stageInvestigationDraft: (draft, generation) => set(state =>
    state.connected && state.generation === generation ? { investigationDraft: draft } : state),
  disconnect: () => set((state) => ({
    workspace: '', situation: '', token: '', connected: false, investigationDraft: '', generation: state.generation + 1,
  })),
}));

export function isCurrentSession(generation) {
  const session = useSessionStore.getState();
  return session.connected && session.generation === generation;
}
