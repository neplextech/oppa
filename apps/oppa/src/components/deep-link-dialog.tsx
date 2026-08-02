import { AlertTriangle, Link2, RefreshCw } from 'lucide-react';
import { useState } from 'react';

import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import type { AgentStatus, DeepLinkPairing } from '@/lib/types';

export function DeepLinkDialog({
  open,
  pairing,
  status,
  busy,
  onConfirm,
  onCancel,
}: {
  open: boolean;
  pairing: DeepLinkPairing | null;
  status: AgentStatus | null;
  busy: string | null;
  onConfirm: (agentName: string) => Promise<void>;
  onCancel: () => void;
}) {
  const [agentName, setAgentName] = useState('My Computer');
  const isCurrentlyPaired = status?.agentId !== null && status?.agentId !== undefined;
  const isPairing = busy === 'pair-server' || busy === 'server-configuration';

  if (!pairing) return null;

  return (
    <Dialog open={open} onOpenChange={(isOpen) => { if (!isOpen) onCancel(); }}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <div className="mb-3 flex size-10 items-center justify-center rounded-full bg-primary/10">
            <Link2 className="size-5 text-primary" aria-hidden />
          </div>
          <DialogTitle>Pair from link</DialogTitle>
          <DialogDescription>
            Connect this computer to the OpenPrinter server at{' '}
            <span className="text-foreground font-mono text-xs">{pairing.serverUrl}</span>.
          </DialogDescription>
        </DialogHeader>

        {isCurrentlyPaired && (
          <div className="flex items-start gap-3 rounded-lg border border-amber-500/20 bg-amber-500/5 p-3">
            <AlertTriangle className="mt-0.5 size-4 shrink-0 text-amber-500" aria-hidden />
            <p className="text-sm text-amber-600 dark:text-amber-400">
              You are currently paired. Confirming will disconnect this computer and require re-pairing.
            </p>
          </div>
        )}

        <div className="space-y-1.5">
          <label className="text-muted-foreground text-xs font-medium">Agent name</label>
          <Input
            value={agentName}
            maxLength={128}
            placeholder="My Computer"
            onChange={(e) => setAgentName(e.target.value)}
            disabled={isPairing}
          />
          <p className="text-muted-foreground/60 text-xs">
            How this computer will be identified on the print server.
          </p>
        </div>

        <DialogFooter>
          <Button type="button" variant="outline" disabled={isPairing} onClick={onCancel}>
            Cancel
          </Button>
          <Button
            type="button"
            disabled={isPairing || agentName.trim().length === 0}
            onClick={() => void onConfirm(agentName.trim())}
          >
            {isPairing ? (
              <>
                <RefreshCw className="size-3.5 animate-spin" aria-hidden />
                Pairing…
              </>
            ) : (
              'Pair now'
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
