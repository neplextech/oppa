import { ExternalLink, LockKeyhole, Power, RotateCcw, ShieldCheck } from 'lucide-react';
import type { ReactNode } from 'react';

import { PageHeader, SectionHeading } from '@/components/page';
import { Badge, Button, Card, Toggle } from '@/components/ui';
import type { AgentStatus } from '@/lib/types';

export function SettingsScreen({
  status,
  busy,
  onStartOnLogin,
  onReconnect,
  onOpenProductLink,
}: {
  status: AgentStatus;
  busy: string | null;
  onStartOnLogin: (enabled: boolean) => Promise<void>;
  onReconnect: () => Promise<void>;
  onOpenProductLink: (link: 'documentation' | 'support', browserFallbackUrl: string) => Promise<void>;
}) {
  return (
    <div className="mx-auto max-w-4xl">
      <PageHeader
        eyebrow="Agent preferences"
        title="Settings"
        description="Manage background behavior and inspect the fixed product configuration compiled into this application."
      />

      <div className="space-y-6">
        <Card className="p-5">
          <SectionHeading title="Background behavior" description="Desktop startup and connectivity" />
          <SettingRow
            icon={<Power className="size-4" aria-hidden />}
            title="Start on login"
            description="Launch the agent after you sign in to this computer."
            control={
              <Toggle
                checked={status.startOnLogin}
                label={`Start ${status.product.name} on login`}
                onChange={(enabled) => void onStartOnLogin(enabled)}
              />
            }
          />
          <SettingRow
            icon={<RotateCcw className="size-4" aria-hidden />}
            title="Reconnect gateway"
            description="Close the active transport cleanly and establish a fresh session."
            control={
              <Button variant="secondary" size="sm" busy={busy === 'reconnect'} onClick={() => void onReconnect()}>
                Reconnect
              </Button>
            }
          />
        </Card>

        <Card className="p-5">
          <SectionHeading
            title="Connected product"
            description="Immutable compile-time product configuration"
            action={<Badge tone="info">Build-time configuration</Badge>}
          />
          <div className="rounded-xl border border-slate-100 bg-slate-50 p-4">
            <div className="flex items-center gap-3">
              <div className="flex size-10 items-center justify-center rounded-xl bg-slate-950 text-white">
                <LockKeyhole className="size-4" aria-hidden />
              </div>
              <div>
                <div className="text-sm font-bold text-slate-900">{status.product.name}</div>
                <div className="mt-0.5 text-xs text-slate-400">{status.product.id}</div>
              </div>
            </div>
            <p className="mt-4 text-xs leading-5 text-slate-500">{status.product.description}</p>
            <div className="mt-4 flex flex-wrap gap-2">
              <button
                type="button"
                className="inline-flex items-center gap-1.5 text-xs font-semibold text-blue-600 hover:text-blue-700"
                onClick={() => void onOpenProductLink('documentation', status.product.documentationUrl)}
              >
                Documentation <ExternalLink className="size-3" aria-hidden />
              </button>
              <span className="text-slate-300">·</span>
              <button
                type="button"
                className="inline-flex items-center gap-1.5 text-xs font-semibold text-blue-600 hover:text-blue-700"
                onClick={() => void onOpenProductLink('support', status.product.supportUrl)}
              >
                Support <ExternalLink className="size-3" aria-hidden />
              </button>
            </div>
          </div>
        </Card>

        <Card className="border-emerald-200 bg-emerald-50/50 p-5">
          <div className="flex gap-3">
            <ShieldCheck className="mt-0.5 size-5 shrink-0 text-emerald-600" aria-hidden />
            <div>
              <h2 className="text-sm font-bold text-emerald-900">Security boundary active</h2>
              <p className="mt-1 text-xs leading-5 text-emerald-700">
                The connected server can send only versioned OpenPrinter messages. It cannot run scripts, browse local
                files, or proxy arbitrary network requests.
              </p>
            </div>
          </div>
        </Card>
      </div>
    </div>
  );
}

function SettingRow({
  icon,
  title,
  description,
  control,
}: {
  icon: ReactNode;
  title: string;
  description: string;
  control: ReactNode;
}) {
  return (
    <div className="flex items-center gap-3 border-t border-slate-100 py-4 first:border-t-0">
      <div className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-slate-100 text-slate-500">
        {icon}
      </div>
      <div className="min-w-0 flex-1">
        <div className="text-sm font-semibold text-slate-700">{title}</div>
        <div className="mt-0.5 text-xs leading-5 text-slate-400">{description}</div>
      </div>
      {control}
    </div>
  );
}
