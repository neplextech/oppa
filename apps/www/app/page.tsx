import {
  ArrowRight,
  ArrowUpRight,
  Github,
  KeyRound,
  Layers,
  Printer,
  Ruler,
  ScrollText,
  ShieldCheck,
  Timer,
} from 'lucide-react';
import Link from 'next/link';

import { CodeBlock } from '@/components/code-block';
import { ProtocolTrace } from '@/components/protocol-trace';
import { SiteHeader } from '@/components/site-header';

const constraints = [
  {
    icon: KeyRound,
    label: 'Loopback auth only',
    detail: 'PKCE and state-bound callbacks bind to ephemeral 127.0.0.1 and expire.',
  },
  {
    icon: ShieldCheck,
    label: 'Credentials never touch SQLite',
    detail: 'OS-native secure storage holds secrets; the local database never sees them.',
  },
  {
    icon: Ruler,
    label: 'Bounded everything',
    detail: 'Message, document, image, and queue sizes are validated at fixed limits.',
  },
  {
    icon: Timer,
    label: 'Explicit timeouts',
    detail: 'Every printer and network call carries a deadline — nothing blocks forever.',
  },
  {
    icon: Layers,
    label: 'Registry-checked printer IDs',
    detail: 'A remote job can only target a printer already enabled in the local registry.',
  },
  {
    icon: ScrollText,
    label: 'No shell, no proxy',
    detail: 'No arbitrary commands, scripts, or generic network proxying — documented messages only.',
  },
];

const serverExample = `const openPrinter = createOpenPrinterServer({
  authenticateAgent: async ({ token }) => {
    const agent = await verifyAgentToken(token);
    return agent ? { agentId: agent.id } : null;
  },
  onJobReceived: ({ agent, message }) => {
    jobs.markReceived(agent.agentId, message.payload.jobId);
  },
});

httpServer.on("upgrade", openPrinter.handleUpgrade);

await openPrinter.sendJob(agentId, job);`;

const installExample = `pnpm install
pnpm --filter openprinter-node-example dev`;

export default function HomePage() {
  return (
    <main className="min-h-screen bg-[#0a0a09] text-stone-100">
      <SiteHeader />

      <section id="trace" className="border-b border-white/10">
        <div className="mx-auto max-w-6xl px-6 pt-16 pb-20 lg:px-8 lg:pt-20 lg:pb-24">
          <div className="grid gap-14 lg:grid-cols-[1fr_1fr] lg:items-start">
            <div>
              <div className="font-mono text-[12px] text-stone-500">OPPA / OPENPRINTER</div>
              <h1 className="mt-5 max-w-lg text-[2.5rem] leading-[1.1] font-semibold tracking-[-0.02em] text-balance sm:text-[2.9rem]">
                A safe bridge between cloud applications and <span className="text-orange-400">local printers</span>.
              </h1>
              <p className="mt-5 max-w-md text-[14px] leading-6 text-stone-400">
                The <span className="text-stone-300">Open Printer Proxy Agent (OPPA)</span> discovers printers and
                processes jobs on the local machine. OpenPrinter gives your server a versioned protocol and a small
                delivery SDK — without owning your queue, database, or business rules.
              </p>

              <div className="mt-8 flex items-center gap-3">
                <Link
                  className="inline-flex h-9 items-center gap-1.5 rounded bg-stone-100 px-4 text-[13px] font-medium text-stone-900 transition hover:bg-white"
                  href="/docs/getting-started"
                >
                  Get started
                  <ArrowRight className="size-3.5" aria-hidden />
                </Link>
                <Link
                  className="inline-flex h-9 items-center gap-1.5 rounded border border-white/10 px-4 text-[13px] font-medium text-stone-300 transition hover:border-white/20 hover:text-stone-100"
                  href="/docs/architecture"
                >
                  Architecture
                </Link>
              </div>
            </div>

            <ProtocolTrace />
          </div>

          <dl className="mt-14 grid grid-cols-1 divide-y divide-white/10 border-y border-white/10 font-mono text-[11px] sm:grid-cols-3 sm:divide-x sm:divide-y-0">
            <div className="px-1 py-4 sm:px-6 sm:first:pl-0">
              <dt className="text-stone-500">delivery</dt>
              <dd className="mt-1 text-[13px] text-stone-300">at-least-once</dd>
            </div>
            <div className="px-1 py-4 sm:px-6">
              <dt className="text-stone-500">ack</dt>
              <dd className="mt-1 text-[13px] text-stone-300">local persistence</dd>
            </div>
            <div className="px-1 py-4 sm:px-6 sm:last:pr-0">
              <dt className="text-stone-500">states</dt>
              <dd className="mt-1 text-[13px] text-stone-300">received / submitted / failed</dd>
            </div>
          </dl>
        </div>
      </section>

      <section id="boundary" className="border-b border-white/10">
        <div className="mx-auto max-w-6xl px-6 py-20 lg:px-8">
          <div className="relative grid gap-6 lg:grid-cols-[1fr_auto] lg:items-center">
            <div className="max-w-lg">
              <h2 className="text-2xl font-semibold tracking-[-0.015em]">
                Two products. One boundary that doesn&apos;t move.
              </h2>
              <p className="mt-3 text-[14px] leading-6 text-stone-400">
                OPPA is the desktop agent with local printer access. OpenPrinter is the protocol and server SDK an
                integrating application talks to. Restaurant, tenant, routing, and billing concepts stay entirely on
                your side of the line.
              </p>
            </div>
            <Printer className="hidden shrink-0 text-white/[0.04] lg:block" strokeWidth={0.75} size={200} aria-hidden />
          </div>

          <div className="mt-12 grid divide-y divide-white/10 border-t border-white/10 md:grid-cols-2 md:divide-x md:divide-y-0">
            <ProductColumn
              index="01"
              title="OPPA"
              role="Local desktop agent"
              description="Discovers printers, persists jobs before acknowledgement, renders documents, submits locally, and reports received, submitted, or failed."
              items={[
                'Tauri host around a shell-independent Rust agent',
                'SQLite job recovery and idempotency',
                'System, raw TCP, and virtual printer boundaries',
                'Compile-time product configuration',
              ]}
              href="/docs/build-oppa"
            />
            <ProductColumn
              index="02"
              title="OpenPrinter"
              role="Protocol and server SDK"
              description="A framework-neutral WebSocket integration with runtime validation, authenticated agent sessions, typed callbacks, and no hidden durable queue."
              items={[
                '@openprinter/protocol for codecs and schemas',
                '@openprinter/server for authenticated sessions',
                'Shared Rust and TypeScript fixtures',
                'At-least-once delivery semantics',
              ]}
              href="/docs/server-sdk"
            />
          </div>
        </div>
      </section>

      <section id="integration" className="border-b border-white/10 bg-white/[0.015]">
        <div className="mx-auto grid max-w-6xl gap-12 px-6 py-20 lg:grid-cols-[0.85fr_1.15fr] lg:items-center lg:px-8">
          <div>
            <h2 className="text-2xl font-semibold tracking-[-0.015em]">
              Bring your own queue, database, and auth policy.
            </h2>
            <p className="mt-3 text-[14px] leading-6 text-stone-400">
              The SDK authenticates the WebSocket session and delivers typed messages. Your application stays
              responsible for job durability, agent authorization, retry policy, and logical printer routing.
            </p>
            <Link
              className="mt-6 inline-flex items-center gap-1.5 text-[13px] font-medium text-orange-400 hover:text-orange-300"
              href="/docs/nodejs-integration"
            >
              See the Node.js integration
              <ArrowUpRight className="size-3.5" aria-hidden />
            </Link>
          </div>
          <CodeBlock code={serverExample} filename="server.ts" highlightedLines={[6, 7, 8]} language="typescript" />
        </div>
      </section>

      <section id="constraints" className="border-b border-white/10">
        <div className="mx-auto max-w-6xl px-6 py-20 lg:px-8">
          <div className="max-w-lg">
            <h2 className="text-2xl font-semibold tracking-[-0.015em]">A deliberately narrow bridge.</h2>
            <p className="mt-3 text-[14px] leading-6 text-stone-400">
              Local printer access is privileged. OPPA accepts only documented messages and exposes no generic remote
              execution surface.
            </p>
          </div>
          <dl className="mt-12 grid gap-x-10 gap-y-8 sm:grid-cols-2 lg:grid-cols-3">
            {constraints.map((item) => {
              const Icon = item.icon;
              return (
                <div key={item.label} className="flex gap-3">
                  <Icon className="mt-0.5 size-4 shrink-0 text-stone-500" aria-hidden />
                  <div>
                    <dt className="text-[13px] font-medium text-stone-200">{item.label}</dt>
                    <dd className="mt-1.5 text-[12.5px] leading-5 text-stone-500">{item.detail}</dd>
                  </div>
                </div>
              );
            })}
          </dl>
        </div>
      </section>

      <section className="border-b border-white/10">
        <div className="mx-auto grid max-w-6xl gap-10 px-6 py-20 lg:grid-cols-[1fr_0.9fr] lg:items-center lg:px-8">
          <div>
            <h2 className="text-2xl font-semibold tracking-[-0.015em]">Start with a virtual printer.</h2>
            <p className="mt-3 max-w-md text-[14px] leading-6 text-stone-400">
              Run the example server, authorize a local agent, and exercise the complete delivery path before connecting
              physical hardware.
            </p>
            <div className="mt-6 flex items-center gap-3">
              <Link
                className="inline-flex h-9 items-center gap-1.5 rounded bg-stone-100 px-4 text-[13px] font-medium text-stone-900 transition hover:bg-white"
                href="/docs/getting-started"
              >
                Open the guide
                <ArrowRight className="size-3.5" aria-hidden />
              </Link>
              <a
                className="inline-flex h-9 items-center gap-1.5 rounded border border-white/10 px-4 text-[13px] font-medium text-stone-300 transition hover:border-white/20 hover:text-stone-100"
                href="https://github.com/neplextech/oppa"
                rel="noreferrer"
                target="_blank"
              >
                <Github className="size-3.5" aria-hidden />
                View source
              </a>
            </div>
          </div>
          <CodeBlock code={installExample} filename="install.sh" language="bash" showLineNumbers={false} />
        </div>
      </section>

      <footer className="py-8">
        <div className="mx-auto flex max-w-6xl flex-col items-center justify-between gap-4 px-6 font-mono text-[11px] text-stone-500 sm:flex-row lg:px-8">
          <span>
            OPPA and OpenPrinter are open-source{' '}
            <a
              className="text-stone-300 underline decoration-stone-700 underline-offset-2 transition hover:text-orange-400 hover:decoration-orange-400"
              href="https://neplextech.com"
              rel="noreferrer"
              target="_blank"
            >
              Neplex
            </a>{' '}
            projects.
          </span>
          <div className="flex gap-5">
            <Link className="hover:text-stone-300" href="/docs/security">
              Security
            </Link>
            <Link className="hover:text-stone-300" href="/docs/contributing">
              Contributing
            </Link>
            <a className="hover:text-stone-300" href="https://github.com/neplextech/oppa/blob/main/LICENSE">
              MIT License
            </a>
          </div>
        </div>
      </footer>
    </main>
  );
}

function ProductColumn({
  index,
  title,
  role,
  description,
  items,
  href,
}: {
  index: string;
  title: string;
  role: string;
  description: string;
  items: string[];
  href: string;
}) {
  return (
    <article className="py-10 first:pt-0 md:px-10 md:py-0 md:first:pl-0 md:last:pr-0">
      <div className="flex items-baseline justify-between">
        <span className="font-mono text-xs text-stone-600">{index}</span>
        <span className="font-mono text-[11px] text-stone-500">{role}</span>
      </div>
      <h3 className="mt-4 text-xl font-semibold tracking-[-0.01em]">{title}</h3>
      <p className="mt-3 text-[13.5px] leading-6 text-stone-400">{description}</p>
      <ul className="mt-6 space-y-2.5 border-t border-white/10 pt-6">
        {items.map((item) => (
          <li key={item} className="flex gap-2.5 font-mono text-[12px] text-stone-400">
            <span className="text-stone-600" aria-hidden>
              &gt;
            </span>
            {item}
          </li>
        ))}
      </ul>
      <Link
        className="mt-7 inline-flex items-center gap-1.5 text-[12.5px] font-medium text-stone-200 hover:text-orange-400"
        href={href}
      >
        Learn more
        <ArrowRight className="size-3.5" aria-hidden />
      </Link>
    </article>
  );
}
