import { ArrowRight, ArrowUpRight, Cable, Printer, ReceiptText } from 'lucide-react';
import type { Metadata } from 'next';
import Link from 'next/link';

import { SiteHeader } from '@/components/site-header';

export const metadata: Metadata = {
  title: 'Neplex OpenPrinter Cloud',
  description: 'Start free with the managed Neplex OpenPrinter Cloud REST API, typed SDK, and OPPA printer delivery.',
};

const SIGNUP_URL = 'https://neplextech.com/signup';

const integrationPaths = [
  {
    icon: ReceiptText,
    title: 'Public REST API',
    description: 'Create durable project jobs and inspect their delivery state with an API key.',
    href: '/docs/openprinter-cloud',
    link: 'Read the REST API reference',
  },
  {
    icon: Cable,
    title: 'TypeScript SDK',
    description: 'Use the strongly typed @openprinter/sdk client with retries, validation, and environment defaults.',
    href: '/docs/openprinter-cloud/sdk',
    link: 'Read the SDK guide',
  },
  {
    icon: Printer,
    title: 'Pair OPPA',
    description: 'Connect a local agent from the Neplex dashboard and deliver jobs to its registered printers.',
    href: '/docs/openprinter-cloud/authentication',
    link: 'Learn about access and pairing',
  },
];

export default function CloudPage() {
  return (
    <main className="min-h-screen bg-[#0a0a09] text-stone-100">
      <SiteHeader />

      <section className="border-b border-white/10">
        <div className="mx-auto max-w-6xl px-6 pt-20 pb-20 lg:px-8 lg:pt-28 lg:pb-24">
          <div className="max-w-3xl">
            <div className="font-mono text-[11px] tracking-[0.16em] text-orange-400">NEPLEX MANAGED SERVICE</div>
            <h1 className="mt-5 text-4xl font-semibold tracking-[-0.035em] sm:text-6xl">Neplex OpenPrinter Cloud</h1>
            <p className="mt-6 max-w-2xl text-[17px] leading-8 text-stone-400">
              A managed REST API for sending durable print jobs to local printers through paired OPPA agents. Start with
              the free tier, then scale the managed service when your workload needs more capacity.
            </p>
            <div className="mt-8 flex flex-wrap items-center gap-4">
              <a
                className="inline-flex h-10 items-center gap-1.5 rounded bg-stone-100 px-5 text-[13px] font-medium text-stone-900 transition hover:bg-white"
                href={SIGNUP_URL}
                rel="noreferrer"
                target="_blank"
              >
                Get started for free
                <ArrowUpRight className="size-3.5" aria-hidden />
              </a>
              <Link
                className="inline-flex h-10 items-center gap-1.5 rounded border border-white/10 px-5 text-[13px] font-medium text-stone-300 transition hover:border-white/20 hover:text-stone-100"
                href="/pricing"
              >
                View managed pricing
                <ArrowRight className="size-3.5" aria-hidden />
              </Link>
            </div>
            <div className="mt-7 flex items-center gap-2 font-mono text-[11px] text-emerald-400">
              <span className="size-1.5 rounded-full bg-emerald-400" aria-hidden />
              Free tier available for getting started
            </div>
          </div>
        </div>
      </section>

      <section className="border-b border-white/10">
        <div className="mx-auto max-w-6xl px-6 py-20 lg:px-8">
          <div className="max-w-xl">
            <div className="font-mono text-[11px] tracking-wide text-stone-500">ONE MANAGED BOUNDARY</div>
            <h2 className="mt-3 text-2xl font-semibold tracking-[-0.02em]">Everything your application needs.</h2>
            <p className="mt-3 text-[14px] leading-7 text-stone-400">
              Neplex OpenPrinter Cloud owns the hosted delivery layer. Your application keeps its business rules, while
              the Cloud API handles project keys, durable jobs, paired agents, and delivery state.
            </p>
          </div>

          <div className="mt-12 grid gap-px overflow-hidden border border-white/10 bg-white/10 md:grid-cols-3">
            {integrationPaths.map((path) => {
              const Icon = path.icon;
              return (
                <article key={path.title} className="bg-[#0a0a09] p-6">
                  <Icon className="size-5 text-orange-400" strokeWidth={1.5} aria-hidden />
                  <h3 className="mt-6 text-lg font-medium">{path.title}</h3>
                  <p className="mt-3 min-h-14 text-[13px] leading-6 text-stone-500">{path.description}</p>
                  <Link
                    className="mt-6 inline-flex items-center gap-1.5 text-[12px] font-medium text-stone-300 hover:text-orange-400"
                    href={path.href}
                  >
                    {path.link}
                    <ArrowRight className="size-3.5" aria-hidden />
                  </Link>
                </article>
              );
            })}
          </div>
        </div>
      </section>

      <section className="border-b border-white/10 bg-white/[0.015]">
        <div className="mx-auto max-w-6xl px-6 py-20 lg:px-8">
          <div className="grid gap-12 lg:grid-cols-[0.8fr_1.2fr] lg:items-start">
            <div>
              <div className="font-mono text-[11px] tracking-wide text-stone-500">HOW IT WORKS</div>
              <h2 className="mt-3 text-2xl font-semibold tracking-[-0.02em]">From project to paper.</h2>
            </div>
            <ol className="divide-y divide-white/10 border-y border-white/10">
              <li className="grid gap-3 py-5 sm:grid-cols-[2rem_1fr]">
                <span className="font-mono text-[12px] text-orange-400">01</span>
                <div>
                  <h3 className="text-[14px] font-medium">Create a project and API key</h3>
                  <p className="mt-2 text-[13px] leading-6 text-stone-500">
                    Use the Neplex dashboard to create a project key with the jobs:write or jobs:read scope your
                    application needs.
                  </p>
                </div>
              </li>
              <li className="grid gap-3 py-5 sm:grid-cols-[2rem_1fr]">
                <span className="font-mono text-[12px] text-orange-400">02</span>
                <div>
                  <h3 className="text-[14px] font-medium">Pair an OPPA agent</h3>
                  <p className="mt-2 text-[13px] leading-6 text-stone-500">
                    Pair the local desktop agent from the dashboard. Its private key stays on the machine; the Cloud
                    service stores only the public identity needed for delivery.
                  </p>
                </div>
              </li>
              <li className="grid gap-3 py-5 sm:grid-cols-[2rem_1fr]">
                <span className="font-mono text-[12px] text-orange-400">03</span>
                <div>
                  <h3 className="text-[14px] font-medium">Submit a durable REST job</h3>
                  <p className="mt-2 text-[13px] leading-6 text-stone-500">
                    Send a structured document with an idempotency key and follow its delivery state from your
                    application.
                  </p>
                </div>
              </li>
            </ol>
          </div>
        </div>
      </section>

      <footer className="mx-auto flex w-full max-w-6xl flex-col gap-3 px-6 py-8 font-mono text-[11px] text-stone-500 sm:flex-row sm:items-center sm:justify-between lg:px-8">
        <span>
          Self-hosting OpenPrinter is completely free. Managed Cloud pricing applies only to Neplex service usage.
        </span>
        <Link className="text-stone-300 hover:text-orange-400" href="/docs/openprinter-cloud">
          Read the public REST API docs
        </Link>
      </footer>
    </main>
  );
}
