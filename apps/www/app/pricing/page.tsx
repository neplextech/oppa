import type { Metadata } from 'next';

import { OpenPrinterPricing } from '@/components/pricing/openprinter-pricing';
import { SiteHeader } from '@/components/site-header';

export const metadata: Metadata = {
  title: 'Pricing',
  description: 'Neplex OpenPrinter Cloud managed API pricing. Self-hosting OpenPrinter is completely free.',
};

export default function PricingPage() {
  return (
    <div className="min-h-screen bg-[#0a0a09]">
      <SiteHeader />
      <OpenPrinterPricing />
    </div>
  );
}
