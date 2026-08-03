import type { BaseLayoutProps } from 'fumadocs-ui/layouts/shared';
import { BookOpen, Download } from 'lucide-react';

import { GithubIcon } from '@/components/site-icons';

export function baseOptions(): BaseLayoutProps {
  return {
    githubUrl: 'https://github.com/neplextech/oppa',
    nav: {
      title: (
        <span className="flex items-center gap-2 font-semibold">
          <span className="bg-fd-primary text-fd-primary-foreground flex size-7 items-center justify-center rounded-lg">
            <BookOpen className="size-3.5" aria-hidden />
          </span>
          OpenPrinter
        </span>
      ),
      url: '/',
    },
    links: [
      {
        icon: <BookOpen />,
        text: 'Documentation',
        url: '/docs',
        active: 'nested-url',
      },
      {
        icon: <Download />,
        text: 'Download OPPA',
        url: '/downloads',
      },
      {
        icon: <GithubIcon />,
        text: 'GitHub',
        url: 'https://github.com/neplextech/oppa',
        external: true,
      },
    ],
  };
}
