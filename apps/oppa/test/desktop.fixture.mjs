/* global location, document */
// Isolated development entry point. Never imported by the production application.
import { agentClient } from '../src/lib/agent-client.ts';
function encodeSvg(value) {
  return btoa(Array.from(new TextEncoder().encode(value), (byte) => String.fromCharCode(byte)).join(''));
}
const options = new URLSearchParams(location.search);
localStorage.removeItem('oppa-onboarding-active');
localStorage.removeItem('oppa-setup-progress');
localStorage.setItem('oppa-theme', options.get('theme') ?? 'light');
localStorage.setItem('oppa-developer-mode', String(!options.has('no-developer')));
localStorage.setItem('oppa-sidebar-collapsed', String(options.has('collapsed')));
localStorage.removeItem('oppa-printer-selection');
localStorage.removeItem('oppa-job-selection');
const status = await agentClient.getStatus();
status.connectionState = options.has('disconnected') ? 'connecting' : 'connected';
status.connectedService = {
  name: 'Office print server',
  serverId: 'office',
  serverVersion: '1.0.0',
  gatewayUrl: 'wss://printer.example.test/gateway',
};
status.pendingJobs = 0;
status.serverConfiguration = { serverUrl: 'https://printer.example.test/' };
let printers = await agentClient.listPrinters();
let jobs = await agentClient.listRecentJobs();
const diagnostics = await agentClient.getDiagnostics();
const office = structuredClone(printers[0]);
office.id = 'office-preview';
office.displayName = 'Office preview';
office.virtualProfile = { type: 'system-driver-page', pageWidthMm: 210, pageHeightMm: 297, dpi: 300 };
office.submissionMode = { type: 'driver' };
office.history = [
  {
    id: 'office-output',
    jobId: 'office-test',
    createdAt: new Date().toISOString(),
    format: 'raster',
    preview: 'Office document test',
    byteLength: 168420,
    imageDataUrl:
      'data:image/svg+xml;base64,' +
      encodeSvg(
        '<svg xmlns="http://www.w3.org/2000/svg" width="794" height="1123"><rect width="794" height="1123" fill="white"/><g font-family="Arial" fill="#202020"><text x="72" y="110" font-size="30">OpenPrinter test document</text><text x="72" y="148" font-size="16" fill="#737373">Office driver · A4 · 210 × 297 mm</text><path d="M72 178H722" stroke="#dddddd"/><text x="72" y="228" font-size="20">Document rendering check</text><text x="72" y="264" font-size="16">This virtual output verifies page proportions and layout.</text><text x="72" y="306" font-size="16">No physical printer was used.</text><rect x="72" y="360" width="650" height="120" rx="0" fill="#fafafa" stroke="#dddddd"/><text x="96" y="406" font-size="16">Page size</text><text x="490" y="406" font-size="16">A4 portrait</text><text x="96" y="446" font-size="16">Submission mode</text><text x="490" y="446" font-size="16">System driver</text><text x="72" y="1040" font-size="13" fill="#737373">Local virtual output · Page 1</text></g></svg>',
      ),
  },
];
const previewImage = document.createElement('img');
previewImage.src = office.history[0].imageDataUrl;
await previewImage.decode();
const previewCanvas = document.createElement('canvas');
previewCanvas.width = 794;
previewCanvas.height = 1123;
previewCanvas.getContext('2d').drawImage(previewImage, 0, 0);
office.history[0].imageDataUrl = previewCanvas.toDataURL('image/png');
office.history[0].pages = [
  { pageNumber: 1, width: 794, height: 1123, dpiX: 96, dpiY: 96, imageDataUrl: office.history[0].imageDataUrl },
  {
    pageNumber: 2,
    width: 794,
    height: 1123,
    dpiX: 96,
    dpiY: 96,
    imageDataUrl: office.history[0].imageDataUrl,
  },
];
office.history[0].pageCount = 2;
printers.push(office);
if (options.has('empty')) {
  printers = [];
  jobs = [];
  diagnostics.logs = [];
}
if (options.has('unavailable')) printers.forEach((p) => (p.available = false));
if (options.has('long'))
  diagnostics.logs.push({
    timestamp: new Date().toISOString(),
    level: 'error',
    target: 'transport',
    message: 'The server connection was interrupted. '.repeat(20),
  });
agentClient.getStatus = async () => structuredClone(status);
agentClient.listPrinters = async () => structuredClone(printers);
agentClient.listRecentJobs = async () => structuredClone(jobs);
agentClient.getDiagnostics = async () => structuredClone(diagnostics);
agentClient.getPendingDeepLink = async () => null;
agentClient.refreshPrinters = async () => structuredClone(printers);
agentClient.configurePrinter = async (id, changes) => {
  const p = printers.find((p) => p.id === id);
  Object.assign(p, changes);
  return structuredClone(p);
};
agentClient.addManualPrinter = async (input) => {
  const p = {
    ...structuredClone(printers[1]),
    id: 'manual-fixture',
    displayName: input.displayName,
    connectionType: 'network',
    address: `${input.host}:${input.port}`,
    submissionMode: { type: 'raw', language: 'esc-pos' },
  };
  printers.push(p);
  return p;
};
agentClient.removePrinter = async (id) => {
  printers = printers.filter((p) => p.id !== id);
};
agentClient.clearJobs = async () => {
  jobs = [];
};
agentClient.clearLogs = async () => {
  diagnostics.logs = [];
};
agentClient.clearVirtualHistory = async (id) => {
  printers.find((p) => p.id === id).history = [];
};
agentClient.updateVirtualPrinter = async (id, mode, delayMs) => {
  const p = printers.find((p) => p.id === id);
  p.mode = mode;
  p.delayMs = delayMs;
  return structuredClone(p);
};
agentClient.reconnect = async () => {
  status.connectionState = 'connected';
  return structuredClone(status);
};
await import('../src/main.tsx');
