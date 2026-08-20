import { Scissors } from 'lucide-react';

import type { PrintBarcodeSection, PrintDocument, PrintSection, PrintTextSection, TextAlignment } from '@/lib/types';
import { cn } from '@/lib/utils';

const textAlignmentClasses: Record<TextAlignment, string> = {
  left: 'text-left',
  center: 'text-center',
  right: 'text-right',
};

const receiptPaperWidths: Record<PrintDocument['width'], string> = {
  58: 'max-w-[300px]',
  80: 'max-w-[390px]',
};

/** Renders a validated structured print document as a paper receipt. */
export function ReceiptPreview({ document, className }: { document: PrintDocument; className?: string }) {
  return (
    <div
      className={cn(
        'receipt-preview flex min-w-0 items-start justify-center overflow-x-auto rounded-md p-3',
        className,
      )}
    >
      <div className={cn('receipt-paper w-full', receiptPaperWidths[document.width])}>
        <div className="receipt-paper__content px-5 py-5 font-mono text-[11px] leading-[1.55]">
          <div className="mb-4 flex items-center justify-between border-b border-dashed border-black/15 pb-3 text-[9px] tracking-[0.16em] text-black/45 uppercase">
            <span>Receipt preview</span>
            <span>{document.width} mm</span>
          </div>
          <div className="flex flex-col gap-1">{document.sections.map(renderSection)}</div>
        </div>
      </div>
    </div>
  );
}

function renderSection(section: PrintSection, index: number) {
  switch (section.type) {
    case 'text':
      return <TextPreview key={`${section.type}-${index}`} section={section} />;
    case 'row':
      return (
        <div key={`${section.type}-${index}`} className="flex items-start justify-between gap-4">
          <span className="min-w-0 break-words whitespace-pre-wrap">{section.left || '\u00a0'}</span>
          <span className="min-w-0 text-right break-words whitespace-pre-wrap">{section.right || '\u00a0'}</span>
        </div>
      );
    case 'divider':
      return <div key={`${section.type}-${index}`} className="my-2 border-t border-dashed border-black/25" />;
    case 'image':
      return (
        <div key={`${section.type}-${index}`} className="my-3 flex justify-center">
          <img
            src={`data:${section.mediaType};base64,${section.data}`}
            alt="Printed document image"
            className="max-h-48 max-w-full object-contain"
            loading="lazy"
          />
        </div>
      );
    case 'qr':
      return <QrPreview key={`${section.type}-${index}`} value={section.value} />;
    case 'barcode':
      return <BarcodePreview key={`${section.type}-${index}`} section={section} />;
    case 'feed':
      return (
        <div
          key={`${section.type}-${index}`}
          aria-label={`Feed ${section.lines} lines`}
          className="border-b border-dotted border-black/15"
          style={{ height: `${Math.min(section.lines, 12) * 8}px` }}
        />
      );
    case 'cut':
      return (
        <div
          key={`${section.type}-${index}`}
          className="my-3 flex items-center gap-2 text-[9px] tracking-[0.18em] text-black/45 uppercase"
        >
          <div className="flex-1 border-t border-dashed border-black/25" />
          <Scissors className="size-3" aria-hidden />
          <span>Cut</span>
          <div className="flex-1 border-t border-dashed border-black/25" />
        </div>
      );
  }
}

function TextPreview({ section }: { section: PrintTextSection }) {
  return (
    <div
      className={cn(
        'break-words whitespace-pre-wrap',
        section.align === undefined ? 'text-left' : textAlignmentClasses[section.align],
        section.bold === true && 'font-bold',
      )}
    >
      {section.value || '\u00a0'}
    </div>
  );
}

function QrPreview({ value }: { value: string }) {
  const matrix = createQrMatrix(value);

  return (
    <div className="my-3 flex flex-col items-center gap-2 text-center text-[9px]">
      <div
        role="img"
        aria-label={`QR code preview for ${value}`}
        className="grid size-32 grid-cols-[repeat(21,minmax(0,1fr))] border-[7px] border-white bg-white p-1"
      >
        {matrix.map((isDark, index) => (
          <span key={index} className={isDark ? 'bg-black' : 'bg-white'} aria-hidden />
        ))}
      </div>
      <span className="max-w-56 break-all text-black/55">{value}</span>
      <span className="tracking-[0.14em] text-black/35 uppercase">QR preview</span>
    </div>
  );
}

function BarcodePreview({ section }: { section: PrintBarcodeSection }) {
  const pattern = createBarcodePattern(section.value, section.format);

  return (
    <div className="my-3 flex flex-col items-center gap-1 text-center text-[9px]">
      <div
        role="img"
        aria-label={`${section.format} barcode preview for ${section.value}`}
        className="flex h-14 w-full max-w-64 items-stretch gap-px bg-white px-4"
      >
        {pattern.map((isDark, index) => (
          <span key={index} className={cn('min-w-px flex-1', isDark ? 'bg-black' : 'bg-white')} aria-hidden />
        ))}
      </div>
      <span className="tracking-[0.08em]">{section.value}</span>
      <span className="tracking-[0.14em] text-black/35 uppercase">{section.format}</span>
    </div>
  );
}

function createQrMatrix(value: string): boolean[] {
  const size = 21;
  const matrix = Array.from({ length: size * size }, () => false);

  for (const [row, column] of [
    [0, 0],
    [0, size - 7],
    [size - 7, 0],
  ] as const) {
    for (let y = -1; y < 8; y += 1) {
      for (let x = -1; x < 8; x += 1) {
        const targetRow = row + y;
        const targetColumn = column + x;
        if (targetRow < 0 || targetColumn < 0 || targetRow >= size || targetColumn >= size) continue;

        const inside = x >= 0 && x < 7 && y >= 0 && y < 7;
        const dark = inside && (x === 0 || x === 6 || y === 0 || y === 6 || (x >= 2 && x <= 4 && y >= 2 && y <= 4));
        matrix[targetRow * size + targetColumn] = dark;
      }
    }
  }

  for (let row = 0; row < size; row += 1) {
    for (let column = 0; column < size; column += 1) {
      if (isQrFinderArea(row, column, size)) continue;
      matrix[row * size + column] = hash(`${value}:${row}:${column}`) % 2 === 0;
    }
  }

  return matrix;
}

function isQrFinderArea(row: number, column: number, size: number): boolean {
  return (row <= 7 && column <= 7) || (row <= 7 && column >= size - 8) || (row >= size - 8 && column <= 7);
}

function createBarcodePattern(value: string, format: PrintBarcodeSection['format']): boolean[] {
  const pattern = [true, false, true, false, false, true];
  const prefix = `${format}:${value}`;

  for (const character of prefix) {
    const code = character.charCodeAt(0);
    for (let bit = 7; bit >= 0; bit -= 1) {
      pattern.push((code & (1 << bit)) !== 0, false);
    }
  }

  return pattern.concat([true, false, true, true, false, true]);
}

function hash(value: string): number {
  let result = 2_166_136_261;
  for (let index = 0; index < value.length; index += 1) {
    result ^= value.charCodeAt(index);
    result = Math.imul(result, 16_777_619);
  }
  return result >>> 0;
}
