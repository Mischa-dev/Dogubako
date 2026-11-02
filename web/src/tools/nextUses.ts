import { Store, useStore } from '../lib/store';

/** When moving to another tool from the tool row: hand over the result, or the original file. */
export type NextUses = 'result' | 'original';
export const nextUses = new Store<NextUses>('dgb.nextUses', 'result', (v: any): v is NextUses => v === 'result' || v === 'original');
export const useNextUses = () => useStore(nextUses);
