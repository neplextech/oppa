/* global location */
// Development-only entry point. Vite's production build includes only index.html.
import { agentClient } from '../src/lib/agent-client.ts';

const options = new URLSearchParams(location.search);
const serverUrl = 'https://printer.example.test/';
const step = options.get('step') ?? 'connect';
if (options.has('existing')) {
  localStorage.removeItem('oppa-onboarding-active');
  localStorage.removeItem('oppa-setup-progress');
} else if (!options.has('resume')) {
  localStorage.setItem('oppa-onboarding-active', 'true');
  localStorage.setItem('oppa-setup-progress', JSON.stringify({ step, serverUrl, computerName: 'Office computer' }));
}
localStorage.setItem('oppa-theme', options.get('theme') ?? 'light');
const status = await agentClient.getStatus();
Object.assign(status, {
  agentId: step === 'printers' || options.has('existing') || options.has('paired') ? 'fixture-computer' : null,
  connectionState: options.get('state') ?? (step === 'printers' ? 'connected' : 'idle'),
  activeErrors: [],
  connectedService: null,
  serverConfiguration: { serverUrl },
});
let printers = await agentClient.listPrinters();
printers = printers.map((printer) => ({ ...printer, enabled: false }));
if (options.has('empty')) printers = [];
if (options.has('long'))
  printers = Array.from({ length: 30 }, (_, index) => ({
    ...printers[0],
    id: `fixture-${index}`,
    displayName: `Office printer ${index + 1}`,
  }));
const identity = {
  name: 'Office print server',
  serverId: 'office-print-server',
  serverVersion: '1.0.0',
  pairingUrl: `${serverUrl}pair`,
  gatewayUrl: 'wss://printer.example.test/gateway',
};
agentClient.getStatus = async () => structuredClone(status);
agentClient.listPrinters = async () => structuredClone(printers);
agentClient.listRecentJobs = async () => [];
agentClient.listRecentServers = async () => [{ serverUrl, name: identity.name, pairedAt: new Date().toISOString() }];
agentClient.discoverServer = async () => {
  if (options.has('discovery-error'))
    throw new Error('This server could not be reached. Check the address and network connection, then try again.');
  return identity;
};
agentClient.setServerConfiguration = async (configuration) => {
  status.serverConfiguration = configuration;
  status.agentId = null;
  return configuration;
};
agentClient.forgetServer = async () => {
  status.agentId = null;
  status.connectionState = 'unpaired';
};
agentClient.pairServer = async () => {
  if (options.has('pair-error')) throw new Error('The pairing code is invalid or has expired.');
  status.agentId = 'fixture-computer';
  status.connectionState = 'connected';
  return identity;
};
agentClient.configurePrinter = async (id, changes) => {
  const printer = printers.find((item) => item.id === id);
  if (!printer) throw new Error('Unknown test printer');
  Object.assign(printer, changes);
  return structuredClone(printer);
};
agentClient.refreshPrinters = async () => structuredClone(printers);
agentClient.getPendingDeepLink = async () => (options.has('deep-link') ? { serverUrl, pairKey: 'LINK-CODE' } : null);
agentClient.reconnect = async () => {
  status.connectionState = 'connected';
  return structuredClone(status);
};
await import('../src/main.tsx');
