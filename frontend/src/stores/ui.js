import { create } from 'zustand';
export const useUiStore = create((set) => ({
  analysis: '', activeOperation: '', denseMode: false,
  // An operation belongs to one analysis. Switching analysis must not keep
  // polling or showing progress from the previously selected analysis.
  setAnalysis: (analysis) => set((state) => ({
    analysis,
    activeOperation: state.analysis === analysis ? state.activeOperation : '',
  })),
  setActiveOperation: (activeOperation) => set({ activeOperation }),
  setDenseMode: (denseMode) => set({ denseMode }),
}));
