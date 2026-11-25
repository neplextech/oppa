import { RefreshCw, Send } from 'lucide-react';
import { useState } from 'react';

import { FieldLabel, inputClass } from '@/components/ui';
import { Button } from '@/components/ui/button';
import { KeyValue } from '@/components/workspace';
import {
  DEFAULT_PDF_PRINT_SETTINGS,
  MAX_TEST_PDF_BYTES,
  type PrinterSummary,
  type PdfCapabilities,
  type PdfPrintSettings,
} from '@/lib/types';
function Row({ label, children }: { label: string; children: string }) {
  return <KeyValue label={label} value={children} />;
}
const PAPER_LABELS: Record<string, string> = { A4: 'A4', A5: 'A5', Letter: 'Letter', Legal: 'Legal' };
const PAGE_RANGE_PATTERN = /^\d{1,5}(-\d{1,5})?(,\d{1,5}(-\d{1,5})?)*$/;

function describeRoute(pdf: PdfCapabilities): string {
  return pdf.route === 'native' ? 'System PDF pipeline' : 'Rasterized by OPPA';
}

/** Shows what a printer accepts for PDFs and submits a local PDF test with explicit settings. */
export function PdfTestSection({
  printer,
  pdf,
  busy,
  onTestPdf,
}: {
  printer: PrinterSummary;
  pdf: PdfCapabilities;
  busy: string | null;
  onTestPdf: (id: string, file: File, settings: PdfPrintSettings) => Promise<void>;
}) {
  const [file, setFile] = useState<File | null>(null);
  const [fileError, setFileError] = useState<string | null>(null);
  const [settings, setSettings] = useState<PdfPrintSettings>(DEFAULT_PDF_PRINT_SETTINGS);
  const [formError, setFormError] = useState<string | null>(null);
  const paperOptions = pdf.paperSizes.filter((size) => size in PAPER_LABELS);
  const canPrint = printer.enabled && printer.available && !!file && busy === null;

  function update<Key extends keyof PdfPrintSettings>(key: Key, value: PdfPrintSettings[Key]) {
    setSettings((current) => ({ ...current, [key]: value }));
    setFormError(null);
  }

  function chooseFile(next: File | null) {
    setFileError(null);
    if (next && (next.size > MAX_TEST_PDF_BYTES || !next.name.toLowerCase().endsWith('.pdf'))) {
      setFile(null);
      setFileError('Choose a PDF file of 20 MB or less.');
      return;
    }
    setFile(next);
  }

  function submit() {
    if (!file) {
      return;
    }
    if (!Number.isInteger(settings.copies) || settings.copies < 1 || settings.copies > 99) {
      setFormError('Copies must be a whole number from 1 to 99.');
      return;
    }
    const ranges = settings.pageRanges.trim();
    if (ranges !== '' && !PAGE_RANGE_PATTERN.test(ranges)) {
      setFormError('Page ranges look like 1-3,5.');
      return;
    }
    if (settings.paper === 'custom') {
      const { customWidthMm: width, customHeightMm: height } = settings;
      if (![width, height].every((value) => Number.isInteger(value) && value >= 10 && value <= 1000)) {
        setFormError('Custom paper dimensions must be whole millimetres from 10 to 1000.');
        return;
      }
    }
    setFormError(null);
    // Failures are shown by the agent hook's error banner, so the rejection is not repeated here.
    onTestPdf(printer.id, file, settings).catch(() => undefined);
  }

  return (
    <div>
      <p className="text-muted-foreground mb-2.5 text-xs font-semibold">PDF documents</p>
      <div className="flex flex-col gap-2">
        <Row label="Route">{describeRoute(pdf)}</Row>
        <Row label="Paper">
          {paperOptions.length > 0
            ? paperOptions.map((size) => PAPER_LABELS[size]).join(', ')
            : 'Driver default (not reported)'}
        </Row>
        <Row label="Custom sizes">{pdf.customPaper ? 'Available' : 'Not available'}</Row>
        <Row label="Duplex">{pdf.duplex ? 'Available' : 'Not available'}</Row>
      </div>

      <div className="mt-4 flex flex-col gap-3">
        <div>
          <FieldLabel htmlFor={`pdf-file-${printer.id}`}>PDF file</FieldLabel>
          <input
            id={`pdf-file-${printer.id}`}
            type="file"
            accept="application/pdf,.pdf"
            className="text-muted-foreground file:bg-secondary block w-full text-xs file:mr-2 file:rounded-md file:border-0 file:px-2 file:py-1 file:text-xs"
            onChange={(event) => chooseFile(event.target.files?.[0] ?? null)}
          />
          {fileError && <p className="text-destructive mt-1 text-xs">{fileError}</p>}
        </div>

        <div className="grid grid-cols-2 gap-2">
          <div>
            <FieldLabel htmlFor={`pdf-copies-${printer.id}`}>Copies</FieldLabel>
            <input
              id={`pdf-copies-${printer.id}`}
              type="number"
              min={1}
              max={99}
              className={inputClass}
              value={settings.copies}
              onChange={(event) => update('copies', Number(event.target.value))}
            />
          </div>
          <div>
            <FieldLabel htmlFor={`pdf-ranges-${printer.id}`}>Pages</FieldLabel>
            <input
              id={`pdf-ranges-${printer.id}`}
              className={inputClass}
              placeholder="All, or 1-3,5"
              value={settings.pageRanges}
              onChange={(event) => update('pageRanges', event.target.value)}
            />
          </div>
        </div>

        <div>
          <FieldLabel htmlFor={`pdf-paper-${printer.id}`}>Paper</FieldLabel>
          <select
            id={`pdf-paper-${printer.id}`}
            className={inputClass}
            value={settings.paper}
            onChange={(event) => update('paper', event.target.value as PdfPrintSettings['paper'])}
          >
            <option value="default">Each page&apos;s own size</option>
            {paperOptions.map((size) => (
              <option key={size} value={size.toLowerCase()}>
                {PAPER_LABELS[size]}
              </option>
            ))}
            {pdf.customPaper && <option value="custom">Custom (labels)</option>}
          </select>
        </div>

        {settings.paper === 'custom' && (
          <div className="grid grid-cols-2 gap-2">
            <div>
              <FieldLabel htmlFor={`pdf-width-${printer.id}`}>Width (mm)</FieldLabel>
              <input
                id={`pdf-width-${printer.id}`}
                type="number"
                min={10}
                max={1000}
                className={inputClass}
                value={settings.customWidthMm}
                onChange={(event) => update('customWidthMm', Number(event.target.value))}
              />
            </div>
            <div>
              <FieldLabel htmlFor={`pdf-height-${printer.id}`}>Height (mm)</FieldLabel>
              <input
                id={`pdf-height-${printer.id}`}
                type="number"
                min={10}
                max={1000}
                className={inputClass}
                value={settings.customHeightMm}
                onChange={(event) => update('customHeightMm', Number(event.target.value))}
              />
            </div>
          </div>
        )}

        <div className="grid grid-cols-2 gap-2">
          <div>
            <FieldLabel htmlFor={`pdf-orientation-${printer.id}`}>Orientation</FieldLabel>
            <select
              id={`pdf-orientation-${printer.id}`}
              className={inputClass}
              value={settings.orientation}
              onChange={(event) => update('orientation', event.target.value as PdfPrintSettings['orientation'])}
            >
              <option value="auto">As document</option>
              <option value="portrait">Portrait</option>
              <option value="landscape">Landscape</option>
            </select>
          </div>
          <div>
            <FieldLabel htmlFor={`pdf-scaling-${printer.id}`}>Scaling</FieldLabel>
            <select
              id={`pdf-scaling-${printer.id}`}
              className={inputClass}
              value={settings.scaling}
              onChange={(event) => update('scaling', event.target.value as PdfPrintSettings['scaling'])}
            >
              <option value="actual">Actual size</option>
              <option value="fit">Fit to paper</option>
            </select>
          </div>
        </div>

        <div>
          <FieldLabel htmlFor={`pdf-duplex-${printer.id}`}>Sides</FieldLabel>
          <select
            id={`pdf-duplex-${printer.id}`}
            className={inputClass}
            value={settings.duplex}
            disabled={!pdf.duplex}
            onChange={(event) => update('duplex', event.target.value as PdfPrintSettings['duplex'])}
          >
            <option value="simplex">One-sided</option>
            {pdf.duplex && <option value="long-edge">Two-sided, long edge</option>}
            {pdf.duplex && <option value="short-edge">Two-sided, short edge</option>}
          </select>
        </div>

        {formError && <p className="text-destructive text-xs">{formError}</p>}

        <Button size="sm" disabled={!canPrint} onClick={submit}>
          {busy === `test-${printer.id}` ? (
            <RefreshCw className="size-3.5 animate-spin" aria-hidden />
          ) : (
            <Send className="size-3.5" aria-hidden />
          )}
          Print PDF test
        </Button>
        <p className="text-muted-foreground text-xs leading-4">
          Submitted to the printer queue. Physical output is not verified.
        </p>
      </div>
    </div>
  );
}
