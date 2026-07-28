import {
  ArrowRight,
  BookOpen,
  CheckCircle2,
  Cloud,
  Code2,
  Github,
  Laptop,
  LockKeyhole,
  Network,
  Printer,
  Server,
  ShieldCheck,
  Waypoints,
} from 'lucide-react';
import Link from 'next/link';

const deliverySteps = [
  {
    label: 'Application',
    detail: 'Owns the durable job and routing decision',
    icon: Cloud,
  },
  {
    label: 'OpenPrinter',
    detail: 'Validates and delivers a versioned message',
    icon: Waypoints,
  },
  {
    label: 'OPPA',
    detail: 'Persists before acknowledging receipt',
    icon: Laptop,
  },
  {
    label: 'Printer',
    detail: 'Receives structured, rendered output',
    icon: Printer,
  },
];

export default function HomePage() {
  return (
    <main className="min-h-screen overflow-hidden bg-white text-slate-950 dark:bg-[#080b10] dark:text-white">
      <div className="landing-grid pointer-events-none absolute inset-x-0 top-0 h-[760px]" />
      <header className="relative z-10 mx-auto flex h-20 max-w-7xl items-center justify-between px-6 lg:px-8">
        <Link className="flex items-center gap-3" href="/">
          <span className="flex size-9 items-center justify-center rounded-xl bg-slate-950 text-white shadow-sm dark:bg-white dark:text-slate-950">
            <Printer className="size-[18px]" strokeWidth={2.2} aria-hidden />
          </span>
          <span>
            <span className="block text-sm font-bold tracking-[-0.02em]">OpenPrinter</span>
            <span className="block text-[10px] font-semibold tracking-[0.13em] text-slate-400 uppercase">
              by Neplex
            </span>
          </span>
        </Link>
        <nav className="hidden items-center gap-7 text-sm font-medium text-slate-500 md:flex dark:text-slate-400">
          <Link className="transition hover:text-slate-950 dark:hover:text-white" href="#architecture">
            Architecture
          </Link>
          <Link className="transition hover:text-slate-950 dark:hover:text-white" href="/docs/protocol">
            Protocol
          </Link>
          <Link className="transition hover:text-slate-950 dark:hover:text-white" href="/docs/server-sdk">
            Server SDK
          </Link>
          <Link className="transition hover:text-slate-950 dark:hover:text-white" href="/docs/security">
            Security
          </Link>
        </nav>
        <div className="flex items-center gap-2">
          <a
            aria-label="Open the OPPA repository on GitHub"
            className="flex size-9 items-center justify-center rounded-lg border border-slate-200 bg-white text-slate-600 transition hover:border-slate-300 hover:text-slate-950 dark:border-white/10 dark:bg-white/5 dark:text-slate-300 dark:hover:text-white"
            href="https://github.com/neplextech/oppa"
            rel="noreferrer"
            target="_blank"
          >
            <Github className="size-4" aria-hidden />
          </a>
          <Link
            className="hidden h-9 items-center gap-2 rounded-lg bg-blue-600 px-4 text-xs font-bold text-white shadow-sm transition hover:bg-blue-700 sm:inline-flex"
            href="/docs"
          >
            Read the docs
            <ArrowRight className="size-3.5" aria-hidden />
          </Link>
        </div>
      </header>

      <section className="relative mx-auto max-w-7xl px-6 pt-24 pb-24 lg:px-8 lg:pt-32 lg:pb-32">
        <div className="mx-auto max-w-4xl text-center">
          <div className="mx-auto mb-7 inline-flex items-center gap-2 rounded-full border border-blue-200 bg-blue-50 px-3 py-1.5 text-[11px] font-bold text-blue-700 dark:border-blue-400/20 dark:bg-blue-400/10 dark:text-blue-300">
            <span className="relative flex size-2">
              <span className="absolute inline-flex size-full animate-ping rounded-full bg-blue-400 opacity-60" />
              <span className="relative inline-flex size-2 rounded-full bg-blue-500" />
            </span>
            Open protocol · local agent · framework-neutral SDK
          </div>
          <h1 className="text-5xl leading-[1.02] font-bold tracking-[-0.055em] text-balance sm:text-6xl lg:text-7xl">
            A safe bridge from cloud applications to <span className="text-blue-600">local printers.</span>
          </h1>
          <p className="mx-auto mt-7 max-w-2xl text-base leading-7 text-pretty text-slate-500 sm:text-lg dark:text-slate-400">
            OPPA discovers printers and processes jobs on the local machine. OpenPrinter gives your server a versioned
            protocol and a small delivery SDK—without owning your queue, database, or business rules.
          </p>
          <div className="mt-9 flex flex-col items-center justify-center gap-3 sm:flex-row">
            <Link
              className="inline-flex h-12 items-center gap-2 rounded-xl bg-slate-950 px-5 text-sm font-bold text-white shadow-lg shadow-slate-950/10 transition hover:-translate-y-0.5 hover:bg-slate-800 dark:bg-white dark:text-slate-950 dark:hover:bg-slate-100"
              href="/docs/getting-started"
            >
              Get started
              <ArrowRight className="size-4" aria-hidden />
            </Link>
            <Link
              className="inline-flex h-12 items-center gap-2 rounded-xl border border-slate-200 bg-white px-5 text-sm font-bold text-slate-700 shadow-sm transition hover:border-slate-300 hover:bg-slate-50 dark:border-white/10 dark:bg-white/5 dark:text-slate-200 dark:hover:bg-white/10"
              href="/docs/architecture"
            >
              <BookOpen className="size-4" aria-hidden />
              Explore the architecture
            </Link>
          </div>
        </div>

        <div className="relative mx-auto mt-20 max-w-5xl">
          <div className="absolute -inset-8 -z-10 rounded-[40px] bg-blue-500/5 blur-3xl dark:bg-blue-400/10" />
          <div className="overflow-hidden rounded-2xl border border-slate-200 bg-white shadow-[0_24px_80px_rgba(15,23,42,0.12)] dark:border-white/10 dark:bg-[#0e131b] dark:shadow-black/40">
            <div className="flex items-center justify-between border-b border-slate-200 bg-slate-50/80 px-5 py-3 dark:border-white/10 dark:bg-white/[0.03]">
              <div className="flex gap-1.5" aria-hidden>
                <span className="size-2.5 rounded-full bg-red-400" />
                <span className="size-2.5 rounded-full bg-amber-400" />
                <span className="size-2.5 rounded-full bg-emerald-400" />
              </div>
              <div className="flex items-center gap-2 text-[10px] font-semibold text-slate-400">
                <span className="size-1.5 rounded-full bg-emerald-500" />
                Agent connected
              </div>
            </div>
            <div className="grid lg:grid-cols-[0.72fr_1.28fr]">
              <div className="border-b border-slate-200 bg-slate-950 p-7 text-white lg:border-r lg:border-b-0 dark:border-white/10">
                <div className="flex items-center gap-3">
                  <div className="flex size-9 items-center justify-center rounded-xl bg-white/10">
                    <Printer className="size-4" aria-hidden />
                  </div>
                  <div>
                    <div className="text-sm font-bold">OPPA</div>
                    <div className="text-[10px] text-slate-400">Open Printer Proxy Agent</div>
                  </div>
                </div>
                <div className="mt-9 space-y-3">
                  {[
                    ['Gateway', 'Connected'],
                    ['Configured printers', '3'],
                    ['Jobs pending', '0'],
                    ['Local database', 'Healthy'],
                  ].map(([label, value]) => (
                    <div key={label} className="flex items-center justify-between border-b border-white/8 pb-3 text-xs">
                      <span className="text-slate-400">{label}</span>
                      <span className="font-semibold text-slate-100">{value}</span>
                    </div>
                  ))}
                </div>
                <div className="mt-7 rounded-xl border border-emerald-400/15 bg-emerald-400/10 p-3 text-[11px] leading-5 text-emerald-200">
                  Job receipt is acknowledged only after durable local persistence.
                </div>
              </div>
              <div className="p-7 lg:p-9">
                <div className="flex items-center justify-between">
                  <div>
                    <div className="text-sm font-bold">Local printer inventory</div>
                    <div className="mt-1 text-xs text-slate-400">Synchronized with the connected application</div>
                  </div>
                  <span className="rounded-full bg-emerald-50 px-2.5 py-1 text-[10px] font-bold text-emerald-700 dark:bg-emerald-400/10 dark:text-emerald-300">
                    2 ready
                  </span>
                </div>
                <div className="mt-6 space-y-3">
                  {[
                    ['Front desk', 'System queue', 'EPSON TM-T82III', true],
                    ['Receipt preview', 'Virtual', 'Development output', true],
                    ['Counter backup', 'Raw TCP', '192.168.1.82:9100', false],
                  ].map(([name, type, detail, ready]) => (
                    <div
                      key={String(name)}
                      className="flex items-center gap-4 rounded-xl border border-slate-200 p-4 dark:border-white/10"
                    >
                      <div className="flex size-9 items-center justify-center rounded-lg bg-slate-100 text-slate-500 dark:bg-white/5 dark:text-slate-300">
                        {type === 'Raw TCP' ? (
                          <Network className="size-4" aria-hidden />
                        ) : (
                          <Printer className="size-4" aria-hidden />
                        )}
                      </div>
                      <div className="min-w-0 flex-1">
                        <div className="text-xs font-bold">{name}</div>
                        <div className="mt-1 truncate text-[10px] text-slate-400">
                          {type} · {detail}
                        </div>
                      </div>
                      <span
                        className={`size-2 rounded-full ${ready ? 'bg-emerald-500' : 'bg-slate-300 dark:bg-slate-600'}`}
                        title={ready ? 'Ready' : 'Unavailable'}
                      />
                    </div>
                  ))}
                </div>
              </div>
            </div>
          </div>
        </div>
      </section>

      <section
        id="architecture"
        className="border-y border-slate-200 bg-slate-50/70 py-24 dark:border-white/10 dark:bg-white/[0.02]"
      >
        <div className="mx-auto max-w-7xl px-6 lg:px-8">
          <div className="grid gap-12 lg:grid-cols-[0.8fr_1.2fr] lg:items-end">
            <div>
              <div className="text-xs font-bold tracking-[0.18em] text-blue-600 uppercase">
                Two products, one boundary
              </div>
              <h2 className="mt-4 text-3xl font-bold tracking-[-0.04em] sm:text-4xl">
                Local execution. Generic integration.
              </h2>
            </div>
            <p className="max-w-2xl text-sm leading-7 text-slate-500 dark:text-slate-400">
              OPPA is the desktop agent with local printer access. OpenPrinter is the protocol and server SDK used by an
              integrating application. Restaurant, tenant, routing, and billing concepts stay entirely in that
              application.
            </p>
          </div>

          <div className="mt-12 grid gap-5 md:grid-cols-2">
            <ProductCard
              eyebrow="Local desktop agent"
              title="OPPA"
              icon={<Laptop className="size-5" aria-hidden />}
              description="Discovers printers, persists jobs before acknowledgement, renders documents, submits locally, and reports received, submitted, or failed."
              items={[
                'Tauri host around a shell-independent Rust agent',
                'SQLite job recovery and idempotency',
                'System, raw TCP, and virtual printer boundaries',
                'Compile-time product configuration',
              ]}
              href="/docs/build-oppa"
            />
            <ProductCard
              eyebrow="Protocol and server SDK"
              title="OpenPrinter"
              icon={<Server className="size-5" aria-hidden />}
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

      <section className="mx-auto max-w-7xl px-6 py-24 lg:px-8">
        <div className="text-center">
          <div className="text-xs font-bold tracking-[0.18em] text-blue-600 uppercase">Delivery semantics</div>
          <h2 className="mt-4 text-3xl font-bold tracking-[-0.04em] sm:text-4xl">
            Every boundary has one clear owner.
          </h2>
          <p className="mx-auto mt-4 max-w-2xl text-sm leading-7 text-slate-500 dark:text-slate-400">
            The application retains work while the agent is offline. The agent makes local receipt durable before
            acknowledging it. No component claims physical printing unless hardware can verify it.
          </p>
        </div>
        <div className="relative mt-14 grid gap-4 md:grid-cols-4">
          <div className="absolute top-7 right-[12.5%] left-[12.5%] hidden h-px bg-slate-200 md:block dark:bg-white/10" />
          {deliverySteps.map((step, index) => {
            const Icon = step.icon;
            return (
              <div
                key={step.label}
                className="relative rounded-2xl border border-slate-200 bg-white p-5 dark:border-white/10 dark:bg-white/[0.03]"
              >
                <div className="relative z-10 flex size-14 items-center justify-center rounded-2xl border border-slate-200 bg-white text-blue-600 shadow-sm dark:border-white/10 dark:bg-[#0e131b]">
                  <Icon className="size-5" aria-hidden />
                </div>
                <div className="mt-5 text-[10px] font-bold tracking-[0.14em] text-slate-400 uppercase">
                  Step {index + 1}
                </div>
                <h3 className="mt-1.5 text-base font-bold">{step.label}</h3>
                <p className="mt-2 text-xs leading-5 text-slate-500 dark:text-slate-400">{step.detail}</p>
              </div>
            );
          })}
        </div>
      </section>

      <section className="bg-slate-950 py-24 text-white">
        <div className="mx-auto grid max-w-7xl gap-12 px-6 lg:grid-cols-[0.8fr_1.2fr] lg:items-center lg:px-8">
          <div>
            <div className="inline-flex items-center gap-2 rounded-full bg-blue-400/10 px-3 py-1.5 text-[11px] font-bold text-blue-300 ring-1 ring-blue-400/20">
              <Code2 className="size-3.5" aria-hidden />
              Small, explicit server API
            </div>
            <h2 className="mt-6 text-3xl font-bold tracking-[-0.04em] sm:text-4xl">
              Bring your own queue, database, and authorization policy.
            </h2>
            <p className="mt-4 text-sm leading-7 text-slate-400">
              The SDK authenticates the WebSocket session and delivers typed messages. Your application remains
              responsible for job durability, agent authorization, retry policy, and logical printer routing.
            </p>
            <Link
              className="mt-7 inline-flex items-center gap-2 text-sm font-bold text-blue-300 hover:text-blue-200"
              href="/docs/nodejs-integration"
            >
              See the Node.js integration
              <ArrowRight className="size-4" aria-hidden />
            </Link>
          </div>
          <div className="overflow-hidden rounded-2xl border border-white/10 bg-[#0a0e15] shadow-2xl shadow-black/30">
            <div className="flex items-center gap-2 border-b border-white/10 px-5 py-3 text-[10px] font-semibold text-slate-500">
              <span className="size-2 rounded-full bg-emerald-400" />
              server.ts
            </div>
            <pre className="overflow-x-auto p-6 text-[12px] leading-6 text-slate-300">
              <code>{`const openPrinter = createOpenPrinterServer({
  authenticateAgent: async ({ token }) => {
    const agent = await verifyAgentToken(token);
    return agent ? { agentId: agent.id } : null;
  },
  onJobReceived: ({ agent, message }) => {
    jobs.markReceived(agent.agentId, message.payload.jobId);
  },
});

httpServer.on("upgrade", openPrinter.handleUpgrade);

await openPrinter.sendJob(agentId, job);`}</code>
            </pre>
          </div>
        </div>
      </section>

      <section className="mx-auto max-w-7xl px-6 py-24 lg:px-8">
        <div className="grid gap-10 rounded-3xl border border-slate-200 bg-slate-50 p-8 sm:p-12 lg:grid-cols-[0.75fr_1.25fr] dark:border-white/10 dark:bg-white/[0.03]">
          <div>
            <div className="flex size-11 items-center justify-center rounded-2xl bg-emerald-100 text-emerald-700 dark:bg-emerald-400/10 dark:text-emerald-300">
              <ShieldCheck className="size-5" aria-hidden />
            </div>
            <h2 className="mt-6 text-2xl font-bold tracking-[-0.035em]">A deliberately narrow bridge</h2>
            <p className="mt-3 text-sm leading-6 text-slate-500 dark:text-slate-400">
              Local printer access is privileged. OPPA accepts only documented messages and exposes no generic remote
              execution surface.
            </p>
          </div>
          <div className="grid gap-3 sm:grid-cols-2">
            {[
              'PKCE and loopback-only authorization callbacks',
              'Credentials stored outside SQLite',
              'Protocol and document size limits',
              'Registered printer identifier validation',
              'Bounded reconnects, queues, and diagnostics',
              'No arbitrary shell, script, file, or proxy commands',
            ].map((item) => (
              <div
                key={item}
                className="flex gap-3 rounded-xl bg-white p-4 text-xs leading-5 shadow-sm ring-1 ring-slate-200/70 dark:bg-white/5 dark:ring-white/10"
              >
                <LockKeyhole className="mt-0.5 size-3.5 shrink-0 text-emerald-600 dark:text-emerald-400" aria-hidden />
                <span className="font-medium text-slate-700 dark:text-slate-300">{item}</span>
              </div>
            ))}
          </div>
        </div>
      </section>

      <section className="border-t border-slate-200 bg-blue-600 py-20 text-white dark:border-white/10">
        <div className="mx-auto flex max-w-7xl flex-col items-center justify-between gap-8 px-6 text-center lg:flex-row lg:px-8 lg:text-left">
          <div>
            <h2 className="text-3xl font-bold tracking-[-0.04em]">Start with a virtual printer.</h2>
            <p className="mt-3 max-w-xl text-sm leading-6 text-blue-100">
              Run the example server, authorize a local agent, and exercise the complete delivery path before connecting
              physical hardware.
            </p>
          </div>
          <div className="flex shrink-0 flex-col gap-3 sm:flex-row">
            <Link
              className="inline-flex h-11 items-center justify-center gap-2 rounded-xl bg-white px-5 text-sm font-bold text-blue-700 shadow-sm transition hover:bg-blue-50"
              href="/docs/getting-started"
            >
              Open the guide
              <ArrowRight className="size-4" aria-hidden />
            </Link>
            <a
              className="inline-flex h-11 items-center justify-center gap-2 rounded-xl bg-blue-700 px-5 text-sm font-bold text-white ring-1 ring-white/20 transition hover:bg-blue-800"
              href="https://github.com/neplextech/oppa"
              rel="noreferrer"
              target="_blank"
            >
              <Github className="size-4" aria-hidden />
              View source
            </a>
          </div>
        </div>
      </section>

      <footer className="bg-slate-950 py-10 text-slate-400">
        <div className="mx-auto flex max-w-7xl flex-col items-center justify-between gap-5 px-6 text-xs sm:flex-row lg:px-8">
          <div className="flex items-center gap-2">
            <Printer className="size-4 text-slate-300" aria-hidden />
            <span>OPPA and OpenPrinter are open-source Neplex projects.</span>
          </div>
          <div className="flex gap-5">
            <Link className="hover:text-white" href="/docs/security">
              Security
            </Link>
            <Link className="hover:text-white" href="/docs/contributing">
              Contributing
            </Link>
            <a className="hover:text-white" href="https://github.com/neplextech/oppa/blob/main/LICENSE">
              MIT License
            </a>
          </div>
        </div>
      </footer>
    </main>
  );
}

function ProductCard({
  eyebrow,
  title,
  icon,
  description,
  items,
  href,
}: {
  eyebrow: string;
  title: string;
  icon: React.ReactNode;
  description: string;
  items: string[];
  href: string;
}) {
  return (
    <article className="group rounded-2xl border border-slate-200 bg-white p-7 shadow-sm transition hover:-translate-y-1 hover:shadow-xl hover:shadow-slate-950/5 dark:border-white/10 dark:bg-white/[0.03]">
      <div className="flex items-start justify-between">
        <div className="flex size-10 items-center justify-center rounded-xl bg-blue-50 text-blue-600 dark:bg-blue-400/10 dark:text-blue-300">
          {icon}
        </div>
        <span className="text-[10px] font-bold tracking-[0.15em] text-slate-400 uppercase">{eyebrow}</span>
      </div>
      <h3 className="mt-7 text-2xl font-bold tracking-[-0.035em]">{title}</h3>
      <p className="mt-3 text-sm leading-6 text-slate-500 dark:text-slate-400">{description}</p>
      <ul className="mt-6 space-y-3">
        {items.map((item) => (
          <li key={item} className="flex gap-2.5 text-xs font-medium text-slate-600 dark:text-slate-300">
            <CheckCircle2 className="mt-px size-3.5 shrink-0 text-emerald-500" aria-hidden />
            {item}
          </li>
        ))}
      </ul>
      <Link
        className="mt-7 inline-flex items-center gap-2 text-xs font-bold text-blue-600 dark:text-blue-300"
        href={href}
      >
        Learn more
        <ArrowRight className="size-3.5 transition-transform group-hover:translate-x-0.5" aria-hidden />
      </Link>
    </article>
  );
}
