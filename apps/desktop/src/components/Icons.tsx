import type { ReactNode, SVGProps } from "react";

// The SF Symbols the macOS app uses, drawn as small inline SVGs (no icon
// dependency). They take the text colour, and are hidden from screen readers:
// the control they sit in carries the label.

type Props = SVGProps<SVGSVGElement> & { size?: number };

function Icon({ size = 14, children, ...props }: Props & { children: ReactNode }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.4}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
      {...props}
    >
      {children}
    </svg>
  );
}

/** `globe`: the Ports view. */
export const GlobeIcon = (props: Props) => (
  <Icon {...props}>
    <circle cx="8" cy="8" r="6.3" />
    <path d="M1.7 8h12.6M8 1.7c1.8 1.7 2.7 3.8 2.7 6.3S9.8 12.6 8 14.3C6.2 12.6 5.3 10.5 5.3 8S6.2 3.4 8 1.7Z" />
  </Icon>
);

export const GearIcon = (props: Props) => (
  <Icon {...props} strokeWidth={1.3}>
    <path d={GEAR} />
    <circle cx="8" cy="8" r="2.1" />
  </Icon>
);

/** Eight teeth around a ring, as `gearshape`. */
const GEAR = (() => {
  const point = (radius: number, angle: number) =>
    `${(8 + radius * Math.cos(angle)).toFixed(2)} ${(8 + radius * Math.sin(angle)).toFixed(2)}`;
  const tooth = Math.PI / 8;
  let path = "";
  for (let i = 0; i < 8; i++) {
    const at = (i * Math.PI) / 4;
    path += `${i === 0 ? "M" : "L"}${point(5, at - tooth * 0.75)} L${point(6.6, at - tooth * 0.4)} L${point(6.6, at + tooth * 0.4)} L${point(5, at + tooth * 0.75)} `;
  }
  return `${path}Z`;
})();

export const PlusIcon = (props: Props) => (
  <Icon {...props}>
    <path d="M8 3v10M3 8h10" />
  </Icon>
);

export const SearchIcon = (props: Props) => (
  <Icon {...props}>
    <circle cx="7" cy="7" r="4.3" />
    <path d="m10.2 10.2 3.3 3.3" />
  </Icon>
);

/** `xmark.circle.fill`: clears a field. */
export const ClearIcon = (props: Props) => (
  <Icon {...props} stroke="none">
    <circle cx="8" cy="8" r="6.5" fill="currentColor" />
    <path d="m5.8 5.8 4.4 4.4m0-4.4-4.4 4.4" stroke="var(--surface-log)" strokeWidth={1.5} />
  </Icon>
);

export const PlayIcon = (props: Props) => (
  <Icon {...props} stroke="none">
    <path d="M4.5 2.8v10.4c0 .5.5.8.9.5l8-5.2a.6.6 0 0 0 0-1L5.4 2.3c-.4-.3-.9 0-.9.5Z" fill="currentColor" />
  </Icon>
);

export const StopIcon = (props: Props) => (
  <Icon {...props} stroke="none">
    <rect x="3.5" y="3.5" width="9" height="9" rx="1.5" fill="currentColor" />
  </Icon>
);

export const EllipsisIcon = (props: Props) => (
  <Icon {...props} stroke="none">
    <circle cx="3.2" cy="8" r="1.25" fill="currentColor" />
    <circle cx="8" cy="8" r="1.25" fill="currentColor" />
    <circle cx="12.8" cy="8" r="1.25" fill="currentColor" />
  </Icon>
);

/** `line.3.horizontal.decrease`: the log filter. */
export const FilterIcon = (props: Props) => (
  <Icon {...props}>
    <path d="M2.5 4.5h11M4.5 8h7M6.5 11.5h3" />
  </Icon>
);

export const ArrowUpRightIcon = (props: Props) => (
  <Icon {...props} strokeWidth={1.8}>
    <path d="M5 11 11 5M6 5h5v5" />
  </Icon>
);

/** `gauge.medium`: a running Service's CPU and memory. */
export const GaugeIcon = (props: Props) => (
  <Icon {...props}>
    <path d="M2.3 11.5a6.3 6.3 0 1 1 11.4 0" />
    <path d="M8 9.5 10.6 6" />
    <circle cx="8" cy="10" r="0.6" fill="currentColor" />
  </Icon>
);

/** `folder`: no Projects. */
export const FolderIcon = (props: Props) => (
  <Icon {...props} strokeWidth={1.1}>
    <path d="M1.8 4.2c0-.7.5-1.2 1.2-1.2h3.2l1.4 1.5h5.4c.7 0 1.2.5 1.2 1.2v6.6c0 .7-.5 1.2-1.2 1.2H3c-.7 0-1.2-.5-1.2-1.2Z" />
    <path d="M1.8 6.3h12.4" />
  </Icon>
);

/** `text.alignleft`: no Service selected. */
export const LinesIcon = (props: Props) => (
  <Icon {...props} strokeWidth={1.1}>
    <path d="M2.5 3.5h11M2.5 6.5h7.5M2.5 9.5h11M2.5 12.5h7.5" />
  </Icon>
);

/** `terminal`: no Services. */
export const TerminalIcon = (props: Props) => (
  <Icon {...props} strokeWidth={1.1}>
    <rect x="1.8" y="2.8" width="12.4" height="10.4" rx="1.6" />
    <path d="m4.6 6.3 2 1.7-2 1.7M8.2 10h3" />
  </Icon>
);

/** `network`: no Listeners. */
export const NetworkIcon = (props: Props) => (
  <Icon {...props} strokeWidth={1.1}>
    <circle cx="8" cy="3.5" r="1.7" />
    <circle cx="3.5" cy="12.5" r="1.7" />
    <circle cx="12.5" cy="12.5" r="1.7" />
    <path d="M8 5.2v3.3M8 8.5 4.6 11.2M8 8.5l3.4 2.7" />
  </Icon>
);

export const CircleHalfIcon = (props: Props) => (
  <Icon {...props}>
    <circle cx="8" cy="8" r="6" />
    <path d="M8 2a6 6 0 0 1 0 12Z" fill="currentColor" />
  </Icon>
);

export const ChevronIcon = (props: Props) => (
  <Icon {...props} strokeWidth={1.8}>
    <path d="m6 3.5 4.5 4.5L6 12.5" />
  </Icon>
);

/** `lock.fill`: a Secret. */
export const LockIcon = (props: Props) => (
  <Icon {...props} strokeWidth={1.3}>
    <rect x="3.3" y="7" width="9.4" height="6.8" rx="1.4" fill="currentColor" />
    <path d="M5.2 7V5.2a2.8 2.8 0 0 1 5.6 0V7" />
  </Icon>
);

/** `lock.open`: a plain variable that could be a Secret. */
export const LockOpenIcon = (props: Props) => (
  <Icon {...props} strokeWidth={1.3}>
    <rect x="3.3" y="7" width="9.4" height="6.8" rx="1.4" />
    <path d="M5.2 7V5.2a2.8 2.8 0 0 1 5.5-.8" />
  </Icon>
);

/** `eye`: reveal a Secret's value. */
export const EyeIcon = (props: Props) => (
  <Icon {...props} strokeWidth={1.3}>
    <path d="M1.5 8S3.8 3.8 8 3.8 14.5 8 14.5 8 12.2 12.2 8 12.2 1.5 8 1.5 8Z" />
    <circle cx="8" cy="8" r="2" />
  </Icon>
);

/** `eye.slash`: hide it again. */
export const EyeSlashIcon = (props: Props) => (
  <Icon {...props} strokeWidth={1.3}>
    <path d="M1.5 8S3.8 3.8 8 3.8 14.5 8 14.5 8 12.2 12.2 8 12.2 1.5 8 1.5 8Z" />
    <circle cx="8" cy="8" r="2" />
    <path d="m2.5 13.5 11-11" />
  </Icon>
);

/** `minus.circle`: remove a row. */
export const MinusCircleIcon = (props: Props) => (
  <Icon {...props} strokeWidth={1.3}>
    <circle cx="8" cy="8" r="6" />
    <path d="M5.2 8h5.6" />
  </Icon>
);

export const TrashIcon = (props: Props) => (
  <Icon {...props} strokeWidth={1.2}>
    <path d="M2.8 4.3h10.4M6.3 4.3V2.8h3.4v1.5M4.2 4.3l.7 9h6.2l.7-9M6.8 6.6v4.6M9.2 6.6v4.6" />
  </Icon>
);

/** `square.3.layers.3d`: the Environment. */
export const LayersIcon = (props: Props) => (
  <Icon {...props} strokeWidth={1.2}>
    <path d="M8 2 14 5 8 8 2 5Z" />
    <path d="m2 8 6 3 6-3M2 11l6 3 6-3" />
  </Icon>
);

/** `sparkle.magnifyingglass`: Detect Services. */
export const DetectIcon = (props: Props) => (
  <Icon {...props} strokeWidth={1.3}>
    <circle cx="6.8" cy="6.8" r="4.6" />
    <path d="m10.2 10.2 3.8 3.8M6.8 4.5l.6 1.7 1.7.6-1.7.6-.6 1.7-.6-1.7-1.7-.6 1.7-.6Z" />
  </Icon>
);

/** `square.and.arrow.down`: Import Project. */
export const ImportIcon = (props: Props) => (
  <Icon {...props} strokeWidth={1.3}>
    <path d="M5.5 5.5H4a1.2 1.2 0 0 0-1.2 1.2v6.1A1.2 1.2 0 0 0 4 14h8a1.2 1.2 0 0 0 1.2-1.2V6.7A1.2 1.2 0 0 0 12 5.5h-1.5M8 1.8v8M5.3 7.3 8 10l2.7-2.7" />
  </Icon>
);

export const CheckIcon = (props: Props) => (
  <Icon {...props} strokeWidth={2}>
    <path d="m3.5 8.3 3 3 6-6.6" />
  </Icon>
);

/** `keyboard`: Keyboard Shortcuts. */
export const KeyboardIcon = (props: Props) => (
  <Icon {...props} strokeWidth={1.2}>
    <rect x="1.5" y="4" width="13" height="8.5" rx="1.6" />
    <path d="M4 6.6h.01M6.3 6.6h.01M8.6 6.6h.01M10.9 6.6h.01M4 8.6h.01M12 6.6h.01M12 8.6h.01M5.2 10.4h5.6" />
  </Icon>
);
