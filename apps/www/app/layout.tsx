import type { Metadata } from 'next';
import { Geist, Geist_Mono } from 'next/font/google';
import type { ReactNode } from 'react';

import './global.css';
import { Providers } from './providers';

const sans = Geist({
  subsets: ['latin'],
  variable: '--font-sans',
});

const mono = Geist_Mono({
  subsets: ['latin'],
  variable: '--font-mono',
});

export const metadata: Metadata = {
  metadataBase: new URL('https://oppa.neplex.dev'),
  icons: {
    apple: '/icon.png',
    icon: '/icon.png',
    shortcut: '/icon.png',
  },
  title: {
    default: 'OpenPrinter — local printing for cloud applications',
    template: '%s · OpenPrinter',
  },
  description:
    'OPPA is the local printer agent. OpenPrinter is the versioned protocol and server SDK that connects it to cloud applications.',
  openGraph: {
    description:
      'OPPA is the local printer agent. OpenPrinter is the versioned protocol and server SDK that connects it to cloud applications.',
    locale: 'en_US',
    siteName: 'OpenPrinter',
    title: 'OpenPrinter — local printing for cloud applications',
    type: 'website',
  },
};

export default function RootLayout({ children }: { children: ReactNode }) {
  return (
    <html lang="en" className={`${sans.variable} ${mono.variable}`} suppressHydrationWarning>
      <body className="flex min-h-screen flex-col">
        <Providers>{children}</Providers>
      </body>
    </html>
  );
}
