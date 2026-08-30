import { ArrowUpRight, Check } from 'lucide-react';
import { cacheLife } from 'next/cache';

const PRICING_URL = 'https://neplextech.com/api/openprinter/pricing';
const SIGNUP_URL = 'https://neplextech.com/signup';

const entitlementLabels: Record<string, string> = {
  'openprinter.agents.max': 'connected agents',
  'openprinter.printers.max': 'registered printers',
  'openprinter.jobs.monthly': 'jobs per month',
  'openprinter.job-retention.days': 'days of job retention',
  'openprinter.api-keys.max': 'project API keys',
  'openprinter.custom-branding.enabled': 'custom branding',
  'openprinter.acknowledgement-webhooks.enabled': 'acknowledgement webhooks',
};

interface PricingProduct {
  readonly name: string;
  readonly description: string | null;
  readonly plans: readonly PricingPlan[];
}

interface PricingPlan {
  readonly slug: string;
  readonly name: string;
  readonly description: string | null;
  readonly isDefaultFree: boolean;
  readonly prices: readonly PricingPrice[];
  readonly entitlements: readonly PricingEntitlement[];
}

interface PricingPrice {
  readonly currency: string;
  readonly amountMinor: number;
  readonly interval: string;
}

interface PricingEntitlement {
  readonly key: string;
  readonly value: unknown;
}

export async function OpenPrinterPricing() {
  const product = await getPricing().catch(() => null);

  return (
    <main className="min-h-screen bg-[#0a0a09] text-stone-100">
      <section className="border-b border-white/10">
        <div className="mx-auto max-w-6xl px-6 pt-20 pb-16 lg:px-8 lg:pt-24">
          <div className="max-w-2xl">
            <div className="font-mono text-[11px] tracking-wide text-orange-400">NEPLEX MANAGED SERVICE</div>
            <h1 className="mt-5 text-4xl font-semibold tracking-[-0.025em] sm:text-5xl">Neplex OpenPrinter Cloud</h1>
            <p className="mt-5 max-w-xl text-[15px] leading-7 text-stone-400">
              Managed project APIs, paired agents, and durable print delivery for teams that want to connect cloud
              applications to local printers without operating the gateway themselves.
            </p>
            <p className="mt-4 text-[13px] leading-6 text-stone-500">
              OpenPrinter Cloud includes a free managed tier so you can get started at no cost. Self-hosting OpenPrinter
              and running OPPA are completely free; paid plans apply only to the Neplex-managed cloud API.
            </p>
          </div>
        </div>
      </section>

      <section className="border-b border-white/10">
        <div className="mx-auto max-w-6xl px-6 py-16 lg:px-8">
          <div className="space-y-8">
            <SelfHostedCard />
            {product ? <ManagedPlans product={product} /> : <FallbackPlans />}
          </div>
        </div>
      </section>

      <section className="border-b border-white/10 bg-white/[0.015]">
        <div className="mx-auto grid max-w-6xl gap-10 px-6 py-16 lg:grid-cols-[1fr_auto] lg:items-center lg:px-8">
          <div>
            <h2 className="text-2xl font-semibold tracking-[-0.015em]">Ready to connect a printer?</h2>
            <p className="mt-3 max-w-xl text-[14px] leading-6 text-stone-400">
              Create a Neplex project, pair OPPA, and use the typed SDK or REST API to send your first durable job.
            </p>
          </div>
          <a
            className="inline-flex h-10 items-center justify-center gap-1.5 rounded bg-stone-100 px-5 text-[13px] font-medium text-stone-900 transition hover:bg-white"
            href={SIGNUP_URL}
            rel="noreferrer"
            target="_blank"
          >
            Get started for free
            <ArrowUpRight className="size-3.5" aria-hidden />
          </a>
        </div>
      </section>

      <footer className="mx-auto flex w-full max-w-6xl flex-col gap-3 px-6 py-8 font-mono text-[11px] text-stone-500 sm:flex-row sm:items-center sm:justify-between lg:px-8">
        <span>Managed cloud pricing only. OpenPrinter self-hosting is completely free.</span>
        <a className="text-stone-300 hover:text-orange-400" href="/cloud">
          Explore OpenPrinter Cloud
        </a>
      </footer>
    </main>
  );
}

function SelfHostedCard() {
  return (
    <article className="rounded border border-emerald-400/25 bg-emerald-400/[0.04] p-6 lg:p-8">
      <div className="grid gap-8 lg:grid-cols-[minmax(0,1fr)_auto_auto] lg:items-center">
        <div>
          <div className="font-mono text-[11px] tracking-wide text-emerald-400">OPEN SOURCE</div>
          <h2 className="mt-3 text-xl font-semibold">Self-hosted OpenPrinter</h2>
          <p className="mt-3 max-w-2xl text-[13px] leading-6 text-stone-400">
            Run the protocol, server SDK, and OPPA yourself with no OpenPrinter license or self-hosting charge.
          </p>
        </div>
        <div className="lg:border-l lg:border-emerald-400/15 lg:pl-8">
          <div className="text-3xl font-semibold tracking-[-0.03em]">$0</div>
          <div className="mt-1 font-mono text-[11px] whitespace-nowrap text-stone-500">completely free, forever</div>
        </div>
        <a
          className="inline-flex items-center gap-1.5 text-[13px] font-medium text-emerald-300 hover:text-emerald-200 lg:justify-self-end"
          href="/docs/getting-started"
        >
          Start self-hosting
          <ArrowUpRight className="size-3.5" aria-hidden />
        </a>
      </div>
    </article>
  );
}

function FallbackPlans() {
  return (
    <div className="rounded border border-orange-400/25 bg-orange-400/[0.04] p-6">
      <div className="font-mono text-[11px] tracking-wide text-orange-400">MANAGED CLOUD PLANS</div>
      <h2 className="mt-3 text-xl font-semibold">Current pricing is temporarily unavailable.</h2>
      <p className="mt-3 text-[13px] leading-6 text-stone-400">
        Check out current pricing at{' '}
        <a
          className="text-orange-300 underline decoration-orange-400/40 underline-offset-4 hover:text-orange-200"
          href={SIGNUP_URL}
          rel="noreferrer"
          target="_blank"
        >
          neplextech.com/signup
        </a>
        .
      </p>
    </div>
  );
}

function ManagedPlans({ product }: { product: PricingProduct }) {
  return (
    <div>
      <div className="mb-5">
        <div className="font-mono text-[11px] tracking-wide text-stone-500">MANAGED CLOUD API</div>
        <p className="mt-2 text-[13px] leading-6 text-stone-400">
          {product.description ?? 'Managed OpenPrinter Cloud plans.'}
        </p>
      </div>
      <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-3">
        {product.plans.map((plan) => (
          <PlanCard key={plan.slug} plan={plan} />
        ))}
      </div>
    </div>
  );
}

function PlanCard({ plan }: { plan: PricingPlan }) {
  const monthly = findPrice(plan.prices, 'MONTH');
  const yearly = findPrice(plan.prices, 'YEAR');
  const highlighted = plan.slug === 'pro';

  return (
    <article
      className={`rounded border p-5 ${highlighted ? 'border-orange-400/45 bg-orange-400/[0.04]' : 'border-white/10 bg-white/[0.02]'}`}
    >
      <div className="flex items-start justify-between gap-3">
        <div>
          <h2 className="text-lg font-semibold">{plan.name}</h2>
          <p className="mt-2 min-h-10 text-[12.5px] leading-5 text-stone-500">{plan.description}</p>
        </div>
        {plan.isDefaultFree && (
          <span className="rounded border border-emerald-400/20 px-2 py-1 font-mono text-[10px] text-emerald-300">
            FREE
          </span>
        )}
      </div>
      <div className="mt-6 border-y border-white/10 py-4">
        <div className="text-2xl font-semibold tracking-[-0.03em]">{monthly ? formatPrice(monthly) : 'Contact us'}</div>
        {monthly && (
          <div className="mt-1 font-mono text-[10px] text-stone-500">
            per month · {yearly ? `${formatPrice(yearly)} billed yearly` : 'managed API'}
          </div>
        )}
      </div>
      <ul className="mt-5 space-y-2.5">
        {plan.entitlements.map((entitlement) => (
          <li key={entitlement.key} className="flex gap-2 text-[12px] leading-5 text-stone-400">
            <Check className="mt-0.5 size-3.5 shrink-0 text-stone-500" aria-hidden />
            <span>{formatEntitlement(entitlement)}</span>
          </li>
        ))}
      </ul>
    </article>
  );
}

function findPrice(prices: readonly PricingPrice[], interval: string): PricingPrice | undefined {
  return prices.find((price) => price.interval.toUpperCase() === interval);
}

function formatPrice(price: PricingPrice): string {
  try {
    return new Intl.NumberFormat('en-US', {
      style: 'currency',
      currency: price.currency,
      maximumFractionDigits: 2,
    }).format(price.amountMinor / 100);
  } catch {
    return `${price.currency} ${(price.amountMinor / 100).toFixed(2)}`;
  }
}

function formatEntitlement(entitlement: PricingEntitlement): string {
  const label =
    entitlementLabels[entitlement.key] ?? entitlement.key.replace(/^openprinter\./, '').replaceAll('.', ' ');
  if (typeof entitlement.value === 'boolean') return entitlement.value ? label : `No ${label}`;
  if (typeof entitlement.value === 'number') return `${entitlement.value.toLocaleString()} ${label}`;
  if (typeof entitlement.value === 'string' && entitlement.value !== '') return `${entitlement.value} ${label}`;
  return label;
}

async function getPricing(): Promise<PricingProduct> {
  'use cache';

  cacheLife('minutes');

  const response = await fetch(PRICING_URL, {
    headers: { accept: 'application/json' },
    signal: AbortSignal.timeout(8_000),
  });
  if (!response.ok) throw new Error(`Pricing request failed with HTTP ${response.status}.`);
  return parsePricingPayload((await response.json()) as unknown);
}

function parsePricingPayload(value: unknown): PricingProduct {
  if (!isRecord(value) || value.apiVersion !== 1 || !isRecord(value.product)) {
    throw new Error('Pricing response has an invalid shape.');
  }
  const product = value.product;
  const plansValue = product.plans;
  if (!Array.isArray(plansValue) || plansValue.length === 0)
    throw new Error('Pricing response has an invalid plan list.');
  if (typeof product.name !== 'string' || product.name === '') throw new Error('Pricing response has no product name.');
  return {
    name: product.name,
    description: nullableString(product.description),
    plans: plansValue.map(parsePlan),
  };
}

function parsePlan(value: unknown): PricingPlan {
  if (
    !isRecord(value) ||
    typeof value.slug !== 'string' ||
    typeof value.name !== 'string' ||
    typeof value.isDefaultFree !== 'boolean' ||
    !Array.isArray(value.prices) ||
    !Array.isArray(value.entitlements)
  ) {
    throw new Error('Pricing response has an invalid plan.');
  }
  return {
    slug: value.slug,
    name: value.name,
    description: nullableString(value.description),
    isDefaultFree: value.isDefaultFree,
    prices: value.prices.map(parsePrice),
    entitlements: value.entitlements.map(parseEntitlement),
  };
}

function parsePrice(value: unknown): PricingPrice {
  if (
    !isRecord(value) ||
    typeof value.currency !== 'string' ||
    typeof value.amountMinor !== 'number' ||
    !Number.isSafeInteger(value.amountMinor) ||
    value.amountMinor < 0 ||
    typeof value.interval !== 'string'
  ) {
    throw new Error('Pricing response has an invalid price.');
  }
  return { currency: value.currency, amountMinor: value.amountMinor, interval: value.interval };
}

function parseEntitlement(value: unknown): PricingEntitlement {
  if (!isRecord(value) || typeof value.key !== 'string')
    throw new Error('Pricing response has an invalid entitlement.');
  return { key: value.key, value: value.value };
}

function nullableString(value: unknown): string | null {
  return value === null || value === undefined ? null : typeof value === 'string' ? value : null;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
