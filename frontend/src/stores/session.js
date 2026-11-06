import { create } from 'zustand';
export const useSessionStore = create((set) => ({
  workspace: '', situation: '', token: '', connected: false,
  connect: ({ workspace, situation, token }) => set({ workspace, situation, token, connected: true }),
  disconnect: () => set({ workspace: '', situation: '', token: '', connected: false }),
}));
