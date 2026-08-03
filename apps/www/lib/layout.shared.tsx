import type { BaseLayoutProps } from 'fumadocs-ui/layouts/shared';
import { BookOpen, Download } from 'lucide-react';

import { SiteBrandIcon } from '@/components/site-brand-icon';
import { GithubIcon } from '@/components/site-icons';

export function baseOptions(): BaseLayoutProps {
  return {
    githubUrl: 'https://github.com/neplextech/oppa',
    nav: {
      title: (
        <span className="flex items-center gap-2 font-semibold">
          <SiteBrandIcon className="size-7 shrink-0" height={28} width={28} />
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
