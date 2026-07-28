import { RootProvider } from 'fumadocs-ui/provider/next';

import SearchDialog from '@/components/search';

export function Providers({ children }: { children: React.ReactNode }) {
  return (
    <RootProvider
      search={{
        SearchDialog,
      }}
    >
      {children}
    </RootProvider>
  );
}
