import { create } from 'zustand';
export const useUiStore = create((set) => ({
  analysis: '', activeOperation: '', denseMode: false,
  setAnalysis: (analysis) => set({ analysis }),
  setActiveOperation: (activeOperation) => set({ activeOperation }),
  setDenseMode: (denseMode) => set({ denseMode }),
}));
