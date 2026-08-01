const IMAGE_BACKGROUND = '#0a0a09';
const PANEL_BACKGROUND = '#10100f';
const BORDER = 'rgba(255, 255, 255, 0.1)';
const MUTED = '#78716c';
const TEXT = '#f5f5f4';
const ORANGE = '#fb923c';
const EMERALD = '#34d399';

export const OPEN_GRAPH_IMAGE_SIZE = {
  width: 1200,
  height: 630,
};

export function OpenPrinterMark({ color = ORANGE }: { color?: string }) {
  return (
    <svg fill="none" height="38" viewBox="0 0 38 38" width="38" xmlns="http://www.w3.org/2000/svg">
      <rect height="23" rx="5" stroke={color} strokeWidth="2" width="30" x="4" y="9" />
      <path d="M10 9V4h18v5M11 25h16v9H11z" stroke={color} strokeLinejoin="round" strokeWidth="2" />
      <circle cx="28" cy="16" fill={EMERALD} r="2" />
    </svg>
  );
}

export function MarketingOpenGraphImage({ variant }: { variant: 'home' | 'downloads' }) {
  const isDownloads = variant === 'downloads';

  return (
    <div
      style={{
        alignItems: 'stretch',
        backgroundColor: IMAGE_BACKGROUND,
        color: TEXT,
        display: 'flex',
        flexDirection: 'column',
        fontFamily: 'sans-serif',
        height: '100%',
        overflow: 'hidden',
        padding: '50px 58px 42px',
        position: 'relative',
        width: '100%',
      }}
    >
      <div
        style={{
          background: `radial-gradient(circle, ${isDownloads ? 'rgba(251, 146, 60, 0.18)' : 'rgba(52, 211, 153, 0.16)'} 0%, rgba(10, 10, 9, 0) 70%)`,
          borderRadius: '999px',
          display: 'flex',
          height: '520px',
          position: 'absolute',
          right: '-130px',
          top: '-180px',
          width: '620px',
        }}
      />
      <div
        style={{
          background: 'radial-gradient(circle, rgba(251, 146, 60, 0.11) 0%, rgba(10, 10, 9, 0) 72%)',
          borderRadius: '999px',
          bottom: '-310px',
          display: 'flex',
          height: '560px',
          left: '-170px',
          position: 'absolute',
          width: '670px',
        }}
      />

      <div
        style={{
          alignItems: 'center',
          display: 'flex',
          flexDirection: 'row',
          justifyContent: 'space-between',
          left: '58px',
          position: 'absolute',
          right: '58px',
          top: '50px',
        }}
      >
        <div style={{ alignItems: 'center', display: 'flex', flexDirection: 'row' }}>
          <div
            style={{
              alignItems: 'center',
              backgroundColor: 'rgba(255, 255, 255, 0.025)',
              border: `1px solid ${BORDER}`,
              borderRadius: '10px',
              display: 'flex',
              height: '54px',
              justifyContent: 'center',
              width: '54px',
            }}
          >
            <OpenPrinterMark />
          </div>
          <div style={{ display: 'flex', flexDirection: 'column', marginLeft: '15px' }}>
            <span style={{ fontSize: '24px', fontWeight: 700, letterSpacing: '-0.5px' }}>OpenPrinter</span>
            <span style={{ color: MUTED, fontSize: '13px', letterSpacing: '2px', marginTop: '2px' }}>BY NEPLEX</span>
          </div>
        </div>
        <div
          style={{
            alignItems: 'center',
            border: `1px solid ${BORDER}`,
            borderRadius: '999px',
            color: '#a8a29e',
            display: 'flex',
            fontSize: '14px',
            letterSpacing: '1.3px',
            padding: '9px 15px',
          }}
        >
          <span
            style={{
              backgroundColor: EMERALD,
              borderRadius: '999px',
              display: 'flex',
              height: '7px',
              marginRight: '9px',
              width: '7px',
            }}
          />
          {isDownloads ? 'OPPA DESKTOP' : 'OPEN PRINTER PROXY AGENT'}
        </div>
      </div>

      <div
        style={{
          alignItems: 'center',
          bottom: '80px',
          display: 'flex',
          flexDirection: 'row',
          justifyContent: 'space-between',
          left: '58px',
          minHeight: 0,
          position: 'absolute',
          right: '58px',
          top: '130px',
        }}
      >
        <div style={{ display: 'flex', flexDirection: 'column', maxWidth: '660px' }}>
          <div style={{ color: MUTED, display: 'flex', fontSize: '14px', letterSpacing: '2.1px' }}>
            {isDownloads ? 'NATIVE INSTALLERS / STABLE CHANNEL' : 'VERSIONED PROTOCOL / LOCAL AGENT'}
          </div>
          {isDownloads ? (
            <div
              style={{
                display: 'flex',
                flexDirection: 'column',
                fontSize: '68px',
                fontWeight: 700,
                letterSpacing: '-3.4px',
                lineHeight: 1.02,
                marginTop: '18px',
              }}
            >
              <span>Your local printers,</span>
              <span style={{ color: ORANGE }}>within reach.</span>
            </div>
          ) : (
            <div
              style={{
                display: 'flex',
                flexDirection: 'column',
                fontSize: '66px',
                fontWeight: 700,
                letterSpacing: '-3.4px',
                lineHeight: 1.02,
                marginTop: '18px',
              }}
            >
              <span style={{ color: EMERALD }}>Cloud applications.</span>
              <span style={{ color: ORANGE }}>Local printers.</span>
            </div>
          )}
          <p
            style={{
              color: '#a8a29e',
              fontSize: '20px',
              lineHeight: 1.5,
              margin: '22px 0 0',
              maxWidth: '610px',
            }}
          >
            {isDownloads
              ? 'Install OPPA on the machine connected to your printers. Available for macOS, Windows, and Linux.'
              : 'One safe, deliberately narrow bridge between cloud applications and the printers they need to reach.'}
          </p>
        </div>

        {isDownloads ? <DownloadPanel /> : <TracePanel />}
      </div>

      <div
        style={{
          alignItems: 'center',
          borderTop: `1px solid ${BORDER}`,
          bottom: '42px',
          color: MUTED,
          display: 'flex',
          flexDirection: 'row',
          fontSize: '13px',
          justifyContent: 'space-between',
          left: '58px',
          letterSpacing: '0.6px',
          paddingTop: '18px',
          position: 'absolute',
          right: '58px',
        }}
      >
        <span>oppa.neplex.dev</span>
        <div style={{ display: 'flex', flexDirection: 'row' }}>
          {(isDownloads ? ['SHA-256 CHECKSUMS', 'SIGNED RELEASES'] : ['AT-LEAST-ONCE', 'BOUNDED', 'RECOVERABLE']).map(
            (label, index) => (
              <span key={label} style={{ marginLeft: index === 0 ? 0 : '26px' }}>
                {label}
              </span>
            ),
          )}
        </div>
      </div>
    </div>
  );
}

function TracePanel() {
  const rows = [
    ['application', 'job.submit', '#78716c'],
    ['openprinter', 'job.deliver', '#78716c'],
    ['oppa', 'job.received', ORANGE],
    ['oppa', 'job.acknowledged', EMERALD],
  ];

  return (
    <div
      style={{
        backgroundColor: PANEL_BACKGROUND,
        border: `1px solid ${BORDER}`,
        borderRadius: '12px',
        display: 'flex',
        flexDirection: 'column',
        marginLeft: '45px',
        overflow: 'hidden',
        width: '390px',
      }}
    >
      <div
        style={{
          borderBottom: `1px solid ${BORDER}`,
          color: MUTED,
          display: 'flex',
          fontSize: '13px',
          justifyContent: 'space-between',
          padding: '15px 18px',
        }}
      >
        <span>protocol trace</span>
        <span style={{ color: EMERALD }}>● live</span>
      </div>
      <div style={{ display: 'flex', flexDirection: 'column', padding: '7px 18px' }}>
        {rows.map(([hop, event, color], index) => (
          <div
            key={event}
            style={{
              alignItems: 'center',
              borderBottom: index === rows.length - 1 ? 'none' : `1px solid rgba(255,255,255,0.055)`,
              display: 'flex',
              fontSize: '13px',
              padding: '13px 0',
            }}
          >
            <span style={{ backgroundColor: color, borderRadius: '99px', height: '7px', width: '7px' }} />
            <span style={{ color: '#d6d3d1', marginLeft: '12px', width: '92px' }}>{hop}</span>
            <span style={{ color: MUTED }}>{event}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

function DownloadPanel() {
  const platforms = [
    ['macOS', 'DMG', ORANGE],
    ['Windows', 'EXE / MSI', '#7dd3fc'],
    ['Linux', 'APPIMAGE / DEB', EMERALD],
  ];

  return (
    <div
      style={{
        backgroundColor: PANEL_BACKGROUND,
        border: `1px solid ${BORDER}`,
        borderRadius: '12px',
        display: 'flex',
        flexDirection: 'column',
        marginLeft: '42px',
        overflow: 'hidden',
        width: '370px',
      }}
    >
      <div
        style={{
          borderBottom: `1px solid ${BORDER}`,
          color: MUTED,
          display: 'flex',
          fontSize: '13px',
          justifyContent: 'space-between',
          padding: '15px 18px',
        }}
      >
        <span>choose a platform</span>
        <span style={{ color: EMERALD }}>stable</span>
      </div>
      <div style={{ display: 'flex', flexDirection: 'column', padding: '6px 18px' }}>
        {platforms.map(([platform, format, color], index) => (
          <div
            key={platform}
            style={{
              alignItems: 'center',
              borderBottom: index === platforms.length - 1 ? 'none' : `1px solid rgba(255,255,255,0.055)`,
              display: 'flex',
              justifyContent: 'space-between',
              padding: '14px 0',
            }}
          >
            <div style={{ alignItems: 'center', display: 'flex', flexDirection: 'row' }}>
              <span
                style={{
                  alignItems: 'center',
                  border: `1px solid ${color}`,
                  borderRadius: '6px',
                  color,
                  display: 'flex',
                  fontSize: '15px',
                  height: '28px',
                  justifyContent: 'center',
                  width: '28px',
                }}
              >
                ↓
              </span>
              <span style={{ color: '#d6d3d1', fontSize: '15px', marginLeft: '12px' }}>{platform}</span>
            </div>
            <span style={{ color: MUTED, fontSize: '11px', letterSpacing: '0.6px' }}>{format}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
