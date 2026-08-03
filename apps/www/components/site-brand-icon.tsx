import type { CSSProperties } from 'react';

export const SITE_BRAND_ICON_SRC = '/icon.png';
export const SITE_BRAND_ICON_OG_SRC = 'https://oppa.neplex.dev/icon.png';

type SiteBrandIconProps = {
  alt?: string;
  className?: string;
  height?: number;
  src?: string;
  style?: CSSProperties;
  width?: number;
};

export function SiteBrandIcon({
  alt = 'OpenPrinter',
  className,
  height = 36,
  src = SITE_BRAND_ICON_SRC,
  style,
  width = 36,
}: SiteBrandIconProps) {
  return (
    <img alt={alt} className={className} height={height} src={src} style={style} width={width} draggable={false} />
  );
}
