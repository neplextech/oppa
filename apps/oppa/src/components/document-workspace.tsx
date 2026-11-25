import { ChevronLeft, ChevronRight, Download, Minus, Plus, SlidersHorizontal } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';

import { ReceiptPreview } from '@/components/receipt-preview';
import { Button } from '@/components/ui/button';
import { KeyValue } from '@/components/workspace';
import type { VirtualOutput, VirtualPrinterSummary } from '@/lib/types';
import { formatRelativeTime } from '@/lib/utils';

/** Local-only captured document inspection, with a canvas that owns zoom and panning. */
export function DocumentWorkspace({ printer }: { printer: VirtualPrinterSummary }) {
  const [selectedId, setSelectedId] = useState('');
  const output = printer.history.find((item) => item.id === selectedId) ?? printer.history[0];
  return (
    <div className="virtual-documents">
      <aside className="capture-history">
        <p className="text-muted-foreground mb-3 text-xs">Captured documents</p>
        {printer.history.map((item) => (
          <button
            key={item.id}
            className="capture-row"
            aria-pressed={output?.id === item.id}
            onClick={() => setSelectedId(item.id)}
          >
            <span className="block text-sm">
              {item.format === 'structured' ? 'Receipt' : item.format === 'esc_pos' ? 'ESC/POS output' : 'Document'}
            </span>
            <span className="text-muted-foreground mt-1 block text-xs">
              {formatRelativeTime(item.createdAt)} · {item.byteLength.toLocaleString()} B
            </span>
          </button>
        ))}
        {!printer.history.length && (
          <p className="text-muted-foreground text-xs leading-5">
            No output captured. Send a test print to inspect rendering.
          </p>
        )}
      </aside>
      {output ? (
        <DocumentCanvas key={output.id} output={output} printer={printer} />
      ) : (
        <div className="bg-muted/40 text-muted-foreground flex min-w-0 flex-1 items-center justify-center p-6 text-center">
          Your captured documents will appear here.
        </div>
      )}
    </div>
  );
}

function DocumentCanvas({ output, printer }: { output: VirtualOutput; printer: VirtualPrinterSummary }) {
  const [failedImage, setFailedImage] = useState<string | null>(null);
  const [pageIndex, setPageIndex] = useState(0);
  const [inspector, setInspector] = useState(false);
  const [zoom, setZoom] = useState(1);
  const [fit, setFit] = useState<'window' | 'width' | 'manual'>('window');
  const [height, setHeight] = useState(300);
  const canvas = useRef<HTMLDivElement>(null);
  const paper = useRef<HTMLDivElement>(null);
  const pan = useRef<{ x: number; y: number; left: number; top: number } | null>(null);
  const page = output.pages?.[pageIndex];
  const imageUrl = page?.imageDataUrl ?? output.imageDataUrl;
  const profile = printer.virtualProfile;
  const widthMm = page
    ? Math.round((page.width / page.dpiX) * 25.4 * 10) / 10
    : profile?.type === 'system-driver-page'
      ? profile.pageWidthMm
      : (profile?.widthMm ?? output.document?.width ?? 80);
  const heightMm = page
    ? Math.round((page.height / page.dpiY) * 25.4 * 10) / 10
    : profile?.type === 'system-driver-page'
      ? profile.pageHeightMm
      : undefined;
  const width = (widthMm * 96) / 25.4;
  const pageTotal = output.pages?.length ?? 1;
  useEffect(() => {
    const host = canvas.current;
    const content = paper.current;
    if (!host || !content) return;
    const update = () => {
      const nextHeight = content.offsetHeight;
      setHeight(nextHeight);
      if (fit !== 'manual')
        setZoom(
          Math.max(
            0.1,
            Math.min(
              4,
              (host.clientWidth - 48) / width,
              fit === 'window' ? (host.clientHeight - 48) / Math.max(nextHeight, 1) : 4,
            ),
          ),
        );
    };
    const observer = new ResizeObserver(update);
    observer.observe(host);
    observer.observe(content);
    update();
    return () => observer.disconnect();
  }, [fit, width, pageIndex, inspector]);
  function exportOutput() {
    const link = document.createElement('a');
    const blob = imageUrl
      ? null
      : new Blob([output.document ? JSON.stringify(output.document, null, 2) : output.preview], {
          type: output.document ? 'application/json' : 'text/plain',
        });
    const url = blob ? URL.createObjectURL(blob) : imageUrl!;
    link.href = url;
    link.download = `oppa-capture-${pageIndex + 1}.${imageUrl ? 'png' : output.document ? 'json' : 'txt'}`;
    link.click();
    if (blob) setTimeout(() => URL.revokeObjectURL(url), 1000);
  }
  const changeZoom = (value: number) => {
    setFit('manual');
    setZoom(Math.max(0.1, Math.min(4, value)));
  };
  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <div className="border-border flex shrink-0 flex-wrap items-center gap-1 border-b px-3 py-2">
        <Button variant="ghost" size="icon-sm" aria-label="Zoom out" onClick={() => changeZoom(zoom - 0.1)}>
          <Minus />
        </Button>
        <span className="text-muted-foreground w-10 text-center text-xs tabular-nums">{Math.round(zoom * 100)}%</span>
        <Button variant="ghost" size="icon-sm" aria-label="Zoom in" onClick={() => changeZoom(zoom + 0.1)}>
          <Plus />
        </Button>
        <select
          aria-label="Preview fit"
          className="bg-background border-border h-8 rounded-md border px-2 text-xs"
          value={fit}
          onChange={(e) => setFit(e.target.value as typeof fit)}
        >
          <option value="window">Fit window</option>
          <option value="width">Fit width</option>
          <option value="manual">Manual zoom</option>
        </select>
        {pageTotal > 1 && (
          <>
            <Button
              variant="ghost"
              size="icon-sm"
              aria-label="Previous page"
              disabled={pageIndex === 0}
              onClick={() => setPageIndex(pageIndex - 1)}
            >
              <ChevronLeft />
            </Button>
            <span className="text-muted-foreground text-xs">
              {page?.pageNumber ?? pageIndex + 1} / {output.pageCount ?? pageTotal}
            </span>
            <Button
              variant="ghost"
              size="icon-sm"
              aria-label="Next page"
              disabled={pageIndex === pageTotal - 1}
              onClick={() => setPageIndex(pageIndex + 1)}
            >
              <ChevronRight />
            </Button>
          </>
        )}
        <span className="flex-1" />
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label="Export captured output"
          title="Export captured output"
          onClick={exportOutput}
        >
          <Download />
        </Button>
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label="Toggle output inspector"
          aria-pressed={inspector}
          onClick={() => setInspector(!inspector)}
        >
          <SlidersHorizontal />
        </Button>
      </div>
      <div className="document-and-inspector">
        <div
          ref={canvas}
          tabIndex={0}
          role="region"
          aria-label="Document preview canvas"
          className="document-canvas"
          onKeyDown={(event) => {
            if (event.key === '+' || event.key === '=') {
              event.preventDefault();
              changeZoom(zoom + 0.1);
            }
            if (event.key === '-') {
              event.preventDefault();
              changeZoom(zoom - 0.1);
            }
            if (event.key === '0') {
              event.preventDefault();
              setFit('window');
            }
          }}
          onPointerDown={(event) => {
            if (event.button !== 0) return;
            const host = event.currentTarget;
            pan.current = { x: event.clientX, y: event.clientY, left: host.scrollLeft, top: host.scrollTop };
            host.setPointerCapture(event.pointerId);
          }}
          onPointerMove={(event) => {
            if (pan.current && event.currentTarget.hasPointerCapture(event.pointerId)) {
              event.currentTarget.scrollLeft = pan.current.left - (event.clientX - pan.current.x);
              event.currentTarget.scrollTop = pan.current.top - (event.clientY - pan.current.y);
            }
          }}
          onPointerUp={(event) => {
            pan.current = null;
            if (event.currentTarget.hasPointerCapture(event.pointerId))
              event.currentTarget.releasePointerCapture(event.pointerId);
          }}
        >
          <div className="mx-auto" style={{ width: width * zoom, height: height * zoom }}>
            <div ref={paper} style={{ width, transform: `scale(${zoom})`, transformOrigin: 'top left' }}>
              {imageUrl && failedImage === imageUrl ? (
                <p role="alert" className="bg-card p-6 text-sm">
                  This preview could not be decoded. Export the captured output or inspect the rendering diagnostics.
                </p>
              ) : imageUrl ? (
                <img
                  onError={() => setFailedImage(imageUrl)}
                  src={imageUrl}
                  alt={`Captured document page ${pageIndex + 1}`}
                  draggable={false}
                  className="block w-full bg-white"
                  style={heightMm ? { aspectRatio: `${widthMm} / ${heightMm}`, objectFit: 'contain' } : undefined}
                />
              ) : output.document ? (
                <ReceiptPreview document={output.document} />
              ) : (
                <pre className="bg-white p-4 font-mono text-xs break-words whitespace-pre-wrap text-black">
                  {output.preview}
                </pre>
              )}
            </div>
          </div>
        </div>
        {inspector && (
          <aside className="output-inspector">
            <h3 className="mb-4 text-sm font-semibold">Captured output</h3>
            <dl className="flex flex-col gap-3">
              <KeyValue label="Format" value={output.format} />
              <KeyValue label="Paper dimensions" value={`${widthMm} mm${heightMm ? ` × ${heightMm} mm` : ' wide'}`} />
              <KeyValue label="Captured" value={new Date(output.createdAt).toLocaleString()} />
              <KeyValue label="Payload size" value={`${output.byteLength.toLocaleString()} bytes`} />
              <KeyValue label="Job ID" value={output.jobId} copy />
            </dl>
            {output.diagnostics && (
              <>
                <h3 className="mt-6 mb-3 text-sm font-semibold">Rendering diagnostics</h3>
                <p className="text-muted-foreground text-xs leading-5">
                  {output.diagnostics.interpretedCommands} commands · {output.diagnostics.textLines} text lines ·{' '}
                  {output.diagnostics.images} images
                  <br />
                  Cut {output.diagnostics.cutRequested ? 'requested' : 'not requested'}
                </p>
                <h4 className="mt-4 mb-2 text-xs font-medium">Unsupported commands</h4>
                <pre className="text-muted-foreground text-xs break-all whitespace-pre-wrap">
                  {output.diagnostics.unsupportedCommands.join('\n') || 'None reported'}
                </pre>
              </>
            )}
            {output.pageCount !== undefined && output.pageCount > pageTotal && (
              <p className="text-muted-foreground mt-4 text-xs">
                {pageTotal} of {output.pageCount} page previews available within the capture budget.
              </p>
            )}
            <details className="mt-5">
              <summary className="cursor-pointer text-xs">Captured payload projection</summary>
              <pre className="mt-3 max-h-64 overflow-auto text-xs break-all whitespace-pre-wrap">
                {output.document ? JSON.stringify(output.document, null, 2) : output.preview}
              </pre>
            </details>
            <p className="text-muted-foreground mt-4 text-xs leading-5">
              Preview scale is approximate. Original binary payloads are not exposed by this inspector.
            </p>
          </aside>
        )}
      </div>
    </div>
  );
}
