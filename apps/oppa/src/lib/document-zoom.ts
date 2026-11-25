/** Preview sizing mode. Actual size uses CSS millimetres at 96 CSS pixels per inch. */
export type PreviewFit = 'actual' | 'window' | 'width' | 'manual';

/** Calculates fit without enlarging a thermal receipt in fit-window mode. */
export function previewZoom(
  mode: PreviewFit,
  viewport: { width: number; height: number },
  paper: { width: number; height: number },
  thermal: boolean,
  manual = 1,
): number {
  if (mode === 'actual') return 1;
  if (mode === 'manual') return Math.max(0.1, Math.min(4, manual));
  const width = Math.max(1, viewport.width - 48) / Math.max(1, paper.width);
  if (mode === 'width') return Math.min(4, width);
  return Math.min(thermal ? 1 : 4, width, Math.max(1, viewport.height - 48) / Math.max(1, paper.height));
}
