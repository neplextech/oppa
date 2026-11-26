import { getTauriVersion } from '@tauri-apps/api/app';
import { ArrowLeft, ExternalLink } from 'lucide-react';
import type { ReactNode } from 'react';
import { useEffect, useState } from 'react';
import { useSearchParams } from 'react-router-dom';

import type { Theme } from '@/App';
import { MobileLifecycleSection } from '@/components/mobile-lifecycle';
import { ServerConfigurationForm } from '@/components/server-configuration-form';
import { ScreenContainer, ScreenHeader, Toggle } from '@/components/ui';
import { Button } from '@/components/ui/button';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import { useUpdater } from '@/components/updater-context';
import { ConfirmAction, KeyValue } from '@/components/workspace';
import { deviceNoun } from '@/lib/platform';
import type { AgentStatus, OpenPrinterServerConfiguration } from '@/lib/types';
export function SettingsScreen({
  status,
  busy,
  theme,
  developerMode,
  fromSetup,
  onThemeChange,
  onDeveloperModeChange,
  onStartOnLogin,
  onReconnect,
  onSaveServerConfiguration,
  onResetServerConfiguration,
  onForgetServer,
  onOpenProductLink,
  onBackToSetup,
}: {
  status: AgentStatus;
  busy: string | null;
  theme: Theme;
  developerMode: boolean;
  fromSetup?: boolean;
  onThemeChange: (t: Theme) => void;
  onDeveloperModeChange: (enabled: boolean) => void;
  onStartOnLogin: (enabled: boolean) => Promise<void>;
  onReconnect: () => Promise<void>;
  onSaveServerConfiguration: (input: OpenPrinterServerConfiguration) => Promise<void>;
  onResetServerConfiguration: () => Promise<void>;
  onForgetServer: () => Promise<void>;
  onOpenProductLink: (link: 'documentation' | 'support', browserFallbackUrl: string) => Promise<void>;
  onBackToSetup?: () => void;
}) {
  const { checkForUpdates, isChecking, lastResult } = useUpdater();
  const capabilities = status.capabilities;
  const device = deviceNoun(capabilities);
  const [params, setParams] = useSearchParams();
  const categories = ['General', 'Connection', 'Appearance', 'Developer', 'About'];
  const category = categories.includes(params.get('category') ?? '') ? params.get('category')! : 'General';
  const [tauriVersion, setTauriVersion] = useState<string | null>(null);
  const [operation, setOperation] = useState<{
    kind: 'forget' | 'save' | 'reset';
    configuration?: OpenPrinterServerConfiguration;
  } | null>(null);
  useEffect(() => {
    if ('__TAURI_INTERNALS__' in window)
      void getTauriVersion()
        .then(setTauriVersion)
        .catch(() => undefined);
  }, []);
  return (
    <ScreenContainer>
      <ScreenHeader
        title="Settings"
        description={`Preferences for ${device}.`}
        action={
          fromSetup && onBackToSetup ? (
            <Button size="sm" variant="ghost" onClick={onBackToSetup}>
              <ArrowLeft />
              Back to setup
            </Button>
          ) : undefined
        }
      />
      <Tabs
        className="min-h-0 flex-1 gap-0"
        value={category}
        onValueChange={(value) => {
          const next = new URLSearchParams(params);
          next.set('category', String(value));
          void setParams(next, { replace: true });
        }}
      >
        <div className="border-border shrink-0 overflow-x-auto border-b px-6 py-2">
          <TabsList variant="line">
            {categories.map((item) => (
              <TabsTrigger key={item} value={item}>
                {item}
              </TabsTrigger>
            ))}
          </TabsList>
        </div>
        <TabsContent value="General" className="min-h-0 overflow-y-auto">
          <div className="preferences-content">
            <PreferenceSection title="Background agent" description="Keep local printing available when you need it.">
              {capabilities.mobile && (
                <MobileLifecycleSection productName={status.product.name} canQuit={capabilities.quit} />
              )}
              {capabilities.startOnLogin && (
                <PreferenceRow
                  title="Start on login"
                  description={`Launch ${status.product.name} when you sign in to ${device}.`}
                  control={
                    <Toggle
                      checked={status.startOnLogin}
                      label="Start on login"
                      onChange={(enabled) => void onStartOnLogin(enabled).catch(() => undefined)}
                    />
                  }
                />
              )}
              {capabilities.systemTray && (
                <PreferenceRow
                  title="Window behavior"
                  description="Closing the window keeps the background agent running. Use Quit to stop the agent."
                />
              )}
            </PreferenceSection>
            {capabilities.updates !== 'none' && (
              <PreferenceSection
                title="Software updates"
                description={
                  capabilities.updates === 'apk_download'
                    ? 'Updates are signed APKs verified against this app before Android asks you to install them.'
                    : 'Signed application updates.'
                }
              >
                <PreferenceRow
                  title="Check for updates"
                  description={lastResult ?? 'Look for a newer signed release.'}
                  control={
                    <Button
                      size="sm"
                      variant="outline"
                      disabled={isChecking}
                      onClick={() => void checkForUpdates().catch(() => undefined)}
                    >
                      {isChecking ? 'Checking…' : 'Check now'}
                    </Button>
                  }
                />
              </PreferenceSection>
            )}
          </div>
        </TabsContent>
        <TabsContent value="Connection" className="min-h-0 overflow-y-auto">
          <div className="preferences-content">
            <PreferenceSection
              title="OpenPrinter server"
              description={`Service identity and the connection for ${device}.`}
            >
              {status.connectedService ? (
                <dl className="flex flex-col gap-2 py-4">
                  <KeyValue label="Service" value={status.connectedService.name} />
                  <KeyValue label="Server ID" value={status.connectedService.serverId} copy />
                  <KeyValue label="Gateway" value={status.connectedService.gatewayUrl} copy />
                </dl>
              ) : (
                <p className="text-muted-foreground py-4 text-sm">
                  Service identity appears after the gateway handshake.
                </p>
              )}
              <ServerConfigurationForm
                value={status.serverConfiguration}
                saving={busy === 'server-configuration'}
                resetting={busy === 'reset-server-configuration'}
                onSave={(configuration) => {
                  setOperation({ kind: 'save', configuration });
                  return Promise.resolve();
                }}
                onReset={() => {
                  setOperation({ kind: 'reset' });
                  return Promise.resolve();
                }}
                compact
              />
              <PreferenceRow
                title="Reconnect"
                description="Establish a fresh gateway connection."
                control={
                  <Button
                    size="sm"
                    variant="outline"
                    disabled={busy !== null}
                    onClick={() => void onReconnect().catch(() => undefined)}
                  >
                    Reconnect
                  </Button>
                }
              />
            </PreferenceSection>
            <PreferenceSection title="Pairing credential" description="Stored securely by the operating system.">
              <PreferenceRow
                title="Forget this server"
                description="Disconnect and remove the saved pairing credential. Pair again to restore access."
                control={
                  <Button
                    size="sm"
                    variant="destructive"
                    disabled={busy !== null}
                    onClick={() => setOperation({ kind: 'forget' })}
                  >
                    Forget server…
                  </Button>
                }
              />
            </PreferenceSection>
          </div>
        </TabsContent>
        <TabsContent value="Appearance" className="min-h-0 overflow-y-auto">
          <div className="preferences-content">
            <PreferenceSection title="Appearance" description="Use a fixed theme or follow your operating system.">
              <PreferenceRow
                title="Color scheme"
                description="Applies to every screen, dialog, and menu."
                control={
                  <select
                    className="preference-select"
                    aria-label="Color scheme"
                    value={theme}
                    onChange={(e) => onThemeChange(e.target.value as Theme)}
                  >
                    <option value="system">System</option>
                    <option value="light">Light</option>
                    <option value="dark">Dark</option>
                  </select>
                }
              />
            </PreferenceSection>
          </div>
        </TabsContent>
        <TabsContent value="Developer" className="min-h-0 overflow-y-auto">
          <div className="preferences-content">
            <PreferenceSection title="Developer tools" description="Tools for local integration testing.">
              <PreferenceRow
                title="Developer Mode"
                description="Reveal the virtual print workspace and captured output inspection."
                control={
                  <Toggle label="Enable Developer Mode" checked={developerMode} onChange={onDeveloperModeChange} />
                }
              />
              {!status.virtualPrinterAvailable && (
                <p className="text-muted-foreground py-4 text-xs">
                  Virtual printing is unavailable in this product build.
                </p>
              )}
            </PreferenceSection>
          </div>
        </TabsContent>
        <TabsContent value="About" className="min-h-0 overflow-y-auto">
          <div className="preferences-content">
            <PreferenceSection title={status.product.name} description={status.product.description}>
              <dl className="flex flex-col gap-3 py-4">
                <KeyValue label="Application version" value={status.version} />
                <KeyValue label="Product ID" value={status.product.id} copy />
                <KeyValue label="Platform" value={status.platform} />
                {tauriVersion && <KeyValue label="Tauri version" value={tauriVersion} />}
                {status.agentId && <KeyValue label="Agent ID" value={status.agentId} copy />}
              </dl>
              <div className="flex gap-3 py-4">
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() =>
                    void onOpenProductLink('documentation', status.product.documentationUrl).catch(() => undefined)
                  }
                >
                  Documentation
                  <ExternalLink />
                </Button>
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => void onOpenProductLink('support', status.product.supportUrl).catch(() => undefined)}
                >
                  Support
                  <ExternalLink />
                </Button>
              </div>
            </PreferenceSection>
          </div>
        </TabsContent>
      </Tabs>
      <ConfirmAction
        open={operation !== null}
        onOpenChange={(open) => {
          if (!open) setOperation(null);
        }}
        title={operation?.kind === 'forget' ? 'Forget server?' : 'Change server configuration?'}
        description={`This disconnects the current server and removes its pairing credential. You will need to verify and pair ${device} again.`}
        busy={busy !== null}
        action={async () => {
          if (operation?.kind === 'forget') await onForgetServer();
          else if (operation?.kind === 'reset') await onResetServerConfiguration();
          else if (operation?.configuration) await onSaveServerConfiguration(operation.configuration);
        }}
      />
    </ScreenContainer>
  );
}
function PreferenceSection({
  title,
  description,
  children,
}: {
  title: string;
  description: string;
  children: ReactNode;
}) {
  return (
    <section className="mb-8">
      <h2 className="text-sm font-semibold">{title}</h2>
      <p className="text-muted-foreground mt-1 mb-3 text-xs leading-5">{description}</p>
      {children}
    </section>
  );
}
function PreferenceRow({ title, description, control }: { title: string; description: string; control?: ReactNode }) {
  return (
    <div className="border-border flex items-center justify-between gap-4 border-b py-4 sm:gap-6">
      <div className="min-w-0">
        <p className="text-sm font-medium">{title}</p>
        <p className="text-muted-foreground mt-1 max-w-lg text-xs leading-5">{description}</p>
      </div>
      {control && <div className="shrink-0">{control}</div>}
    </div>
  );
}
