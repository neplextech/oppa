'use client';

import { useEffect, useRef, useState } from 'react';

type Tone = 'neutral' | 'accent' | 'success';

interface Step {
  hop: string;
  event: string;
  status: string;
  offset: string;
  tone: Tone;
}

const steps: Step[] = [
  {
    hop: 'application',
    event: 'job.submit',
    status: 'queued',
    offset: '+0ms',
    tone: 'neutral',
  },
  {
    hop: 'openprinter',
    event: 'job.deliver',
    status: 'delivered',
    offset: '+17ms',
    tone: 'neutral',
  },
  {
    hop: 'oppa',
    event: 'job.received',
    status: 'received',
    offset: '+43ms',
    tone: 'accent',
  },
  {
    hop: 'oppa',
    event: 'job.acknowledged',
    status: 'persisted',
    offset: '+1501ms',
    tone: 'success',
  },
];

const toneDot: Record<Tone, string> = {
  neutral: 'bg-stone-500',
  accent: 'bg-orange-400',
  success: 'bg-emerald-400',
};

const toneText: Record<Tone, string> = {
  neutral: 'text-stone-400',
  accent: 'text-orange-400',
  success: 'text-emerald-400',
};

const START_JOB = 9247;
const STEP_INTERVAL = 650;
const START_DELAY = 900;

export function ProtocolTrace() {
  const [revealed, setRevealed] = useState(0);
  const [live, setLive] = useState(false);
  const timeouts = useRef<ReturnType<typeof setTimeout>[]>([]);

  useEffect(() => {
    let cancelled = false;

    function schedule(fn: () => void, delay: number) {
      const id = setTimeout(() => {
        if (!cancelled) fn();
      }, delay);
      timeouts.current.push(id);
    }

    steps.forEach((_, i) => {
      schedule(() => setRevealed(i + 1), START_DELAY + (i + 1) * STEP_INTERVAL);
    });
    schedule(() => setLive(true), START_DELAY + steps.length * STEP_INTERVAL);

    return () => {
      cancelled = true;
      for (const id of timeouts.current) clearTimeout(id);
      timeouts.current = [];
    };
  }, []);

  return (
    <div className="rounded border border-white/10 bg-[#0c0c0b]">
      <div className="flex items-center justify-between border-b border-white/10 px-4 py-3">
        <span className="font-mono text-[11px] text-stone-500">protocol trace</span>
        <span className="flex items-center gap-1.5 font-mono text-[11px] text-stone-500">
          <span className="relative flex size-1.5">
            {live && (
              <span className="absolute inline-flex size-full animate-ping rounded-full bg-emerald-400 opacity-75" />
            )}
            <span
              className={`relative inline-flex size-1.5 rounded-full ${live ? 'bg-emerald-400' : 'bg-stone-600'}`}
            />
          </span>
          job_{START_JOB}
        </span>
      </div>

      <ol className="ml-[19px] border-l border-white/10">
        {steps.map((step, i) => {
          const active = i < revealed;
          return (
            <li key={step.event} className="relative py-3 pl-5">
              <span
                className={`absolute top-[1.15rem] -left-[4px] size-[7px] rounded-full ring-4 ring-[#0c0c0b] transition-colors duration-500 ${
                  active ? toneDot[step.tone] : 'bg-stone-700'
                }`}
                aria-hidden
              />
              <div
                className={`flex items-center justify-between gap-3 pr-4 transition-all duration-500 ${
                  active ? 'translate-y-0 opacity-100' : 'translate-y-1 opacity-40'
                }`}
              >
                <div className="flex items-baseline gap-3">
                  <span className="w-14 font-mono text-[11px] text-stone-600 tabular-nums">
                    {active ? step.offset : '—'}
                  </span>
                  <span className="text-[13px] font-medium text-stone-200">{step.hop}</span>
                  <span className="font-mono text-[11px] text-stone-500">{step.event}</span>
                </div>
                <span
                  className={`font-mono text-[11px] transition-colors duration-500 ${active ? toneText[step.tone] : 'text-stone-700'}`}
                >
                  {active ? step.status : 'pending'}
                </span>
              </div>
            </li>
          );
        })}
      </ol>

      <p className="border-t border-white/10 px-4 py-3 font-mono text-[11px] leading-5 text-stone-500">
        Application retains the job while OPPA is offline. Receipt is acknowledged only after durable local persistence.
      </p>
    </div>
  );
}
