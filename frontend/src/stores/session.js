import { create } from 'zustand';
export const useSessionStore = create((set) => ({
  workspace: '', situation: '', token: '', connected: false, generation: 0,
  // A monotonically increasing session generation scopes local component drafts
  // even when credentials change for the same workspace and Situation.
  connect: ({ workspace, situation, token }) => set((state) => ({
    workspace, situation, token, connected: true, generation: state.generation + 1,
  })),
  disconnect: () => set((state) => ({
    workspace: '', situation: '', token: '', connected: false, generation: state.generation + 1,
  })),
}));