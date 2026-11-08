import { describe,it,expect,beforeEach } from 'vitest';import { useSessionStore } from './session.js';import { useUiStore } from './ui.js';
describe('client state',()=>{beforeEach(()=>{useSessionStore.getState().disconnect();useUiStore.setState({analysis:'',activeOperation:'',denseMode:false});});it('keeps credentials in explicit session state without persistence magic',()=>{useSessionStore.getState().connect({workspace:'w',situation:'s',token:'secret'});expect(useSessionStore.getState().connected).toBe(true);useSessionStore.getState().disconnect();expect(useSessionStore.getState().token).toBe('');});it('tracks navigation-adjacent UI state separately from server state',()=>{useUiStore.getState().setAnalysis('a');useUiStore.getState().setActiveOperation('o');expect(useUiStore.getState().analysis).toBe('a');expect(useUiStore.getState().activeOperation).toBe('o');});});

it('clears stale operation when a different analysis is selected',()=>{
  useUiStore.getState().setAnalysis('first');
  useUiStore.getState().setActiveOperation('running-first');
  useUiStore.getState().setAnalysis('first');
  expect(useUiStore.getState().activeOperation).toBe('running-first');
  useUiStore.getState().setAnalysis('second');
  expect(useUiStore.getState().analysis).toBe('second');
  expect(useUiStore.getState().activeOperation).toBe('');
});
