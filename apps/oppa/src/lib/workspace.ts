import type { KeyboardEvent } from 'react';
import { useSearchParams } from 'react-router-dom';
/** A durable selection follows explicit deep navigation and otherwise restores the previous inspector. */
export function useSelection(storageKey: string) {
  const [params, setParams] = useSearchParams();
  const selected = params.has('selected') ? params.get('selected') : localStorage.getItem(storageKey);
  function select(id: string | null) {
    if (id) localStorage.setItem(storageKey, id);
    else localStorage.removeItem(storageKey);
    const next = new URLSearchParams(params);
    next.set('selected', id ?? '');
    void setParams(next, { replace: true });
  }
  return [selected || null, select] as const;
}

/** Arrow, Home, and End navigation for compact selectable rows; interactive child controls retain their keys. */
export function listNavigation(event: KeyboardEvent<HTMLElement>, onSelect: (id: string) => void) {
  if (event.target !== event.currentTarget) return;
  const rows = Array.from(event.currentTarget.parentElement?.querySelectorAll<HTMLElement>('[data-row-id]') ?? []);
  const index = rows.indexOf(event.currentTarget);
  const next =
    event.key === 'ArrowDown'
      ? rows[index + 1]
      : event.key === 'ArrowUp'
        ? rows[index - 1]
        : event.key === 'Home'
          ? rows[0]
          : event.key === 'End'
            ? rows[rows.length - 1]
            : undefined;
  if (next) {
    event.preventDefault();
    next.focus();
    onSelect(next.dataset.rowId!);
  }
  if (event.key === 'Enter' || event.key === ' ') {
    event.preventDefault();
    onSelect(event.currentTarget.dataset.rowId!);
  }
}
