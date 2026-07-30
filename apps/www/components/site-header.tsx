'use client';

import { ArrowRight, Github, Menu, X } from 'lucide-react';
import Link from 'next/link';
import { usePathname } from 'next/navigation';
import { useEffect, useState } from 'react';

const navItems = [
  { label: 'Protocol', href: '/#trace', sectionId: 'trace' },
  { label: 'Boundary', href: '/#boundary', sectionId: 'boundary' },
  { label: 'Integration', href: '/#integration', sectionId: 'integration' },
  { label: 'Security', href: '/#constraints', sectionId: 'constraints' },
  { label: 'Downloads', href: '/downloads', sectionId: null },
];

export function SiteHeader() {
  const pathname = usePathname();
  const [scrolled, setScrolled] = useState(false);
  const [active, setActive] = useState<string>('');
  const [menuOpen, setMenuOpen] = useState(false);

  useEffect(() => {
    const onScroll = () => setScrolled(window.scrollY > 8);
    onScroll();
    window.addEventListener('scroll', onScroll, { passive: true });
    return () => window.removeEventListener('scroll', onScroll);
  }, []);

  useEffect(() => {
    if (pathname !== '/') {
      setActive('');
      return;
    }

    const sections = navItems
      .filter((item) => item.sectionId !== null)
      .map((item) => document.getElementById(item.sectionId ?? ''))
      .filter((element): element is HTMLElement => element !== null);

    if (sections.length === 0) return;

    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) {
            setActive(entry.target.id);
          }
        }
      },
      { rootMargin: '-40% 0px -55% 0px', threshold: 0 },
    );

    for (const section of sections) observer.observe(section);
    return () => observer.disconnect();
  }, [pathname]);

  return (
    <header
      className={`sticky top-0 z-50 border-b transition-colors ${
        scrolled || menuOpen ? 'border-white/10 bg-[#0a0a09]/85 backdrop-blur-md' : 'border-transparent bg-transparent'
      }`}
    >
      <div className="mx-auto flex h-14 max-w-6xl items-center justify-between px-6 lg:px-8">
        <Link className="flex items-center gap-2.5" href="/">
          <img
            alt="Neplex"
            className="h-9 w-auto invert"
            height={36}
            src="https://neplextech.com/neplex-transparent-200.webp"
            width={36}
          />
          <span className="leading-[1.15]">
            <span className="block text-[13.5px] font-semibold tracking-tight text-stone-100">OpenPrinter</span>
            <span className="block font-mono text-[9.5px] tracking-wide text-stone-500">by Neplex</span>
          </span>
        </Link>

        <nav className="hidden items-center gap-6 font-mono text-[12.5px] md:flex">
          {navItems.map((item) => (
            <Link
              key={item.href}
              className={`transition ${
                (item.sectionId !== null && active === item.sectionId) ||
                (item.sectionId === null && pathname.startsWith(item.href))
                  ? 'text-stone-100'
                  : 'text-stone-500 hover:text-stone-300'
              }`}
              href={item.href}
            >
              {item.label}
            </Link>
          ))}
        </nav>

        <div className="flex items-center gap-2">
          <a
            aria-label="Open the OPPA repository on GitHub"
            className="flex size-8 items-center justify-center rounded border border-white/10 text-stone-400 transition hover:border-white/20 hover:text-stone-100"
            href="https://github.com/neplextech/oppa"
            rel="noreferrer"
            target="_blank"
          >
            <Github className="size-[15px]" aria-hidden />
          </a>
          <Link
            className="hidden h-8 items-center gap-1.5 rounded border border-white/10 bg-white/[0.04] px-3 text-[12.5px] font-medium text-stone-200 transition hover:border-white/20 hover:bg-white/[0.08] sm:inline-flex"
            href="/docs"
          >
            Read the docs
            <ArrowRight className="size-3.5" aria-hidden />
          </Link>
          <button
            aria-expanded={menuOpen}
            aria-label="Toggle menu"
            className="flex size-8 items-center justify-center rounded border border-white/10 text-stone-300 transition hover:border-white/20 hover:text-stone-100 md:hidden"
            onClick={() => setMenuOpen((v) => !v)}
            type="button"
          >
            {menuOpen ? <X className="size-4" aria-hidden /> : <Menu className="size-4" aria-hidden />}
          </button>
        </div>
      </div>

      {menuOpen && (
        <div className="border-t border-white/10 px-6 py-4 md:hidden">
          <nav className="flex flex-col gap-4 font-mono text-[13px]">
            {navItems.map((item) => (
              <Link
                key={item.href}
                className={`transition ${
                  item.sectionId === null && pathname.startsWith(item.href)
                    ? 'text-stone-100'
                    : 'text-stone-300 hover:text-stone-100'
                }`}
                href={item.href}
                onClick={() => setMenuOpen(false)}
              >
                {item.label}
              </Link>
            ))}
            <Link className="flex items-center gap-1.5 text-stone-100" href="/docs" onClick={() => setMenuOpen(false)}>
              Read the docs
              <ArrowRight className="size-3.5" aria-hidden />
            </Link>
          </nav>
        </div>
      )}
    </header>
  );
}
