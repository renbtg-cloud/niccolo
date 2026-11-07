import { useEffect } from 'react';
import { useAnalysisHistory } from './useAnalysisHistory.js';
import { useUiStore } from '../stores/ui.js';

export function latestAnalysisId(history) {
  return history?.analyses?.[0]?.analysis_id ?? '';
}

export function useLatestAnalysisSelection() {
  const history = useAnalysisHistory();
  const analysis = useUiStore((state) => state.analysis);
  const setAnalysis = useUiStore((state) => state.setAnalysis);
  useEffect(() => {
    if (!analysis) {
      const latest = latestAnalysisId(history.data);
      if (latest) setAnalysis(latest);
    }
  }, [analysis, history.data, setAnalysis]);
  return history;
}
