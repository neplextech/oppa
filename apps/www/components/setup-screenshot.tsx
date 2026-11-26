import Image from 'next/image';

interface SetupScreenshotProps {
  /** File name of a dark-mode OPPA capture in the setup image directory. */
  name: string;
  /** Describes the controls and state illustrated by this screenshot. */
  alt: string;
  /** Explains what the reader should notice in the application. */
  children: React.ReactNode;
  /** Uses the portrait dimensions of an Android application capture. */
  mobile?: boolean;
}

/** Displays an application screenshot with a caption and a full-size view. */
export function SetupScreenshot({ name, alt, children, mobile = false }: SetupScreenshotProps) {
  const src = `/images/setup/${name}-dark.png`;
  return (
    <figure className={mobile ? 'mx-auto my-8 max-w-[340px]' : 'my-8'}>
      <a href={src} target="_blank" rel="noreferrer" aria-label={`Enlarge screenshot: ${alt}`}>
        <Image
          src={src}
          alt={alt}
          width={mobile ? 1080 : 960}
          height={mobile ? 2220 : 640}
          sizes={mobile ? '340px' : '(max-width: 768px) 100vw, 800px'}
          className="!m-0 h-auto w-full rounded-xl border border-white/10"
        />
      </a>
      <figcaption className="text-fd-muted-foreground mt-3 text-sm leading-6">{children}</figcaption>
    </figure>
  );
}
