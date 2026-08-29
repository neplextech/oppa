interface ScreenshotPlaceholderProps {
  /** Short name for the future dashboard screenshot. */
  label: string;
  /** What the screenshot should demonstrate when the real capture is added. */
  description: string;
}

/** Reserved visual slot for private Neplex dashboard screenshots. */
export function ScreenshotPlaceholder({ label, description }: ScreenshotPlaceholderProps) {
  return (
    <figure className="not-prose border-fd-border bg-fd-muted/30 my-8 overflow-hidden rounded-xl border border-dashed">
      <div className="flex min-h-56 items-center justify-center bg-[linear-gradient(135deg,transparent_25%,color-mix(in_srgb,var(--color-fd-border)_35%,transparent)_25%,color-mix(in_srgb,var(--color-fd-border)_35%,transparent)_50%,transparent_50%,transparent_75%,color-mix(in_srgb,var(--color-fd-border)_35%,transparent)_75%)] bg-size-[24px_24px] px-6 text-center">
        <div>
          <p className="text-fd-muted-foreground font-mono text-[11px] font-medium tracking-[0.18em] uppercase">
            Screenshot placeholder
          </p>
          <p className="text-fd-foreground mt-2 text-sm font-medium">{label}</p>
        </div>
      </div>
      <figcaption className="border-fd-border text-fd-muted-foreground border-t border-dashed px-4 py-3 text-xs leading-5">
        {description}
      </figcaption>
    </figure>
  );
}
