import { type CSSProperties, type ReactNode, type RefObject, useEffect, useRef, useState } from "react";
import { commands, type Id, type Project, type RunStats, type Service } from "../bindings";
import { type CoreView, serviceStatus } from "../core";
import { isActive } from "../status";
import { GaugeIcon } from "./Icons";
import { serviceColor } from "./ServicesColumn";
import styles from "./Stats.module.css";

/** Stats are sampled only while a stats popover is open, this often (ADR-0012). */
export const STATS_INTERVAL = 1_500;
/** How many samples the sparklines keep: the last minute. */
export const HISTORY_LENGTH = 40;

/** One Run's samples since its stats were opened. */
export type StatsHistory = { samples: RunStats[]; peakMemory: number };

/** Adds a sample; a sample of another Run starts over, `null` (no active Run) drops the history. */
export function appendSample(history: StatsHistory | undefined, stats: RunStats | null): StatsHistory | undefined {
  if (!stats) return undefined;
  const previous = history?.samples.at(-1)?.runId === stats.runId ? history : undefined;
  return {
    samples: [...(previous?.samples ?? []), stats].slice(-HISTORY_LENGTH),
    peakMemory: Math.max(previous?.peakMemory ?? 0, stats.memoryBytes),
  };
}

/** Samples the Services' active Runs every `STATS_INTERVAL` while mounted, each with its previous sample. */
export function useRunStats(serviceIds: Id[]): Map<Id, StatsHistory> {
  const [histories, setHistories] = useState(() => new Map<Id, StatsHistory>());
  const key = serviceIds.join(",");
  useEffect(() => {
    let isCancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let current = new Map<Id, StatsHistory>();
    const sample = async () => {
      const sampled = await Promise.all(
        serviceIds.map(async (serviceId) => {
          const result = await commands.runStats(serviceId, current.get(serviceId)?.samples.at(-1) ?? null);
          return [serviceId, appendSample(current.get(serviceId), result.status === "ok" ? result.data : null)] as const;
        }),
      );
      if (isCancelled) return;
      current = new Map(sampled.filter((entry): entry is [Id, StatsHistory] => entry[1] !== undefined));
      setHistories(current);
      timer = setTimeout(() => void sample(), STATS_INTERVAL);
    };
    void sample();
    return () => {
      isCancelled = true;
      clearTimeout(timer);
    };
    // The ids are in `key`.
  }, [key]);
  return histories;
}

/** CPU use where 100% is one core; "–" until there are two samples. */
export function formatCpu(percent: number | null | undefined): string {
  if (percent == null) return "–";
  return `${percent.toFixed(percent < 10 ? 1 : 0)}%`;
}

/** "512 KB", "84.2 MB", "1.3 GB", in units of 1024 as system monitors show memory. */
export function formatMemory(bytes: number): string {
  const units = ["bytes", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return unit === 0 ? `${value} bytes` : `${value.toFixed(value < 10 || unit >= 2 ? 1 : 0)} ${units[unit]}`;
}

/** The gauge button in a running Service's log header: its CPU and memory in a popover. */
export function ServiceStatsButton({ service, index }: { service: Service; index: number }) {
  const [isOpen, setIsOpen] = useState(false);
  const button = useRef<HTMLButtonElement>(null);
  return (
    <span className={styles.anchor}>
      <button
        ref={button}
        className="iconButton"
        title={`CPU and memory of ${service.name}`}
        aria-label={`Show CPU and Memory of ${service.name}`}
        aria-expanded={isOpen}
        onClick={(event) => {
          event.stopPropagation();
          setIsOpen(!isOpen);
        }}
      >
        <GaugeIcon size={14} />
      </button>
      {isOpen && (
        <Popover
          label={`CPU and memory of ${service.name}`}
          anchor={button}
          onClose={(returnFocus) => {
            setIsOpen(false);
            if (returnFocus) button.current?.focus();
          }}
        >
          <ServiceStats service={service} index={index} />
        </Popover>
      )}
    </span>
  );
}

function ServiceStats({ service, index }: { service: Service; index: number }) {
  const history = useRunStats([service.id]).get(service.id);
  const [hasSampled, setHasSampled] = useState(false);
  useEffect(() => {
    const timer = setTimeout(() => setHasSampled(true), STATS_INTERVAL);
    return () => clearTimeout(timer);
  }, []);
  const latest = history?.samples.at(-1);
  const color = serviceColor(index);
  return (
    <div className={styles.content} style={{ "--service": color } as CSSProperties}>
      <div className={styles.title}>{service.name}</div>
      {latest && history ? (
        <>
          <StatRow
            title="CPU"
            value={formatCpu(latest.cpuPercent)}
            help="100% is one core fully busy"
            values={history.samples.flatMap((sample) => (sample.cpuPercent == null ? [] : [sample.cpuPercent]))}
            floor={100}
          />
          <StatRow
            title="Memory"
            value={formatMemory(latest.memoryBytes)}
            help="Memory of the Run's processes, counting what they share once"
            values={history.samples.map((sample) => sample.memoryBytes)}
            floor={0}
          />
          <dl className={styles.facts}>
            <dt>Processes</dt>
            <dd>{latest.processCount}</dd>
            <dt title="The most memory used since this was opened">Peak memory</dt>
            <dd>{formatMemory(history.peakMemory)}</dd>
          </dl>
        </>
      ) : (
        <div className="secondary">{hasSampled ? "Not running." : "Measuring…"}</div>
      )}
    </div>
  );
}

/** A label, its current value, and a sparkline of its recent values. */
function StatRow(props: { title: string; value: string; help: string; values: number[]; floor: number }) {
  const { title, value, help, values, floor } = props;
  return (
    <div className={styles.stat} title={help} role="group" aria-label={`${title} ${value}`}>
      <div className={styles.statLine} aria-hidden="true">
        <span className="secondary">{title}</span>
        <span className={styles.value}>{value}</span>
      </div>
      <Sparkline values={values} top={Math.max(floor, Math.max(0, ...values) * 1.1)} />
    </div>
  );
}

/** Recent values as a line, the newest at the right edge, filling leftwards. */
export function Sparkline({ values, top }: { values: number[]; top: number }) {
  const [width, height] = [100, 28];
  const step = width / (HISTORY_LENGTH - 1);
  const points = values.map((value, index) => {
    const x = width - (values.length - 1 - index) * step;
    const y = height * (1 - Math.min(value, top) / (top || 1));
    return `${x.toFixed(2)},${y.toFixed(2)}`;
  });
  const first = points[0]?.split(",")[0];
  return (
    <svg className={styles.sparkline} viewBox={`0 0 ${width} ${height}`} preserveAspectRatio="none" aria-hidden="true">
      {values.length > 1 && (
        <>
          <polygon className={styles.area} points={`${first},${height} ${points.join(" ")} ${width},${height}`} />
          <polyline className={styles.line} points={points.join(" ")} />
        </>
      )}
    </svg>
  );
}

/** "Stats…" in a Project's `⋯` menu: each running Service's CPU and memory, and the total. */
export function ProjectStats({ project, view, onClose }: { project: Project; view: CoreView; onClose: () => void }) {
  const active = project.services.filter((service) => isActive(serviceStatus(view, service.id).state));
  const histories = useRunStats(active.map((service) => service.id));
  const measured = project.services.flatMap((service, index) => {
    const latest = histories.get(service.id)?.samples.at(-1);
    return latest ? [{ service, index, stats: latest }] : [];
  });
  const cpus = measured.map(({ stats }) => stats.cpuPercent);
  const totalCpu = cpus.some((cpu) => cpu == null) ? null : cpus.reduce<number>((sum, cpu) => sum + (cpu ?? 0), 0);
  return (
    <Popover label={`Stats of ${project.name}`} onClose={onClose}>
      <div className={styles.content}>
        <div className={styles.title}>{project.name}</div>
        {measured.length === 0 ? (
          <div className="secondary">No services running.</div>
        ) : (
          <table className={styles.table}>
            <thead>
              <tr className="secondary">
                <th scope="col">Service</th>
                <th scope="col">CPU</th>
                <th scope="col">Memory</th>
              </tr>
            </thead>
            <tbody>
              {measured.map(({ service, index, stats }) => (
                <tr key={service.id}>
                  <th scope="row">
                    <span
                      className={styles.swatch}
                      style={{ "--service": serviceColor(index) } as CSSProperties}
                      aria-hidden="true"
                    />
                    {service.name}
                  </th>
                  <td>{formatCpu(stats.cpuPercent)}</td>
                  <td>{formatMemory(stats.memoryBytes)}</td>
                </tr>
              ))}
            </tbody>
            <tfoot>
              <tr>
                <th scope="row">Total</th>
                <td>{formatCpu(totalCpu)}</td>
                <td>{formatMemory(measured.reduce((sum, { stats }) => sum + stats.memoryBytes, 0))}</td>
              </tr>
            </tfoot>
          </table>
        )}
      </div>
    </Popover>
  );
}

/** A panel below its anchor. Escape closes it (giving focus back), as does a click elsewhere. */
function Popover({
  label,
  anchor,
  onClose,
  children,
}: {
  label: string;
  /** The button that toggles it, whose clicks it leaves alone. */
  anchor?: RefObject<HTMLElement | null>;
  /** `true` when closed from the keyboard, so focus can go back. */
  onClose: (returnFocus: boolean) => void;
  children: ReactNode;
}) {
  const panel = useRef<HTMLDivElement>(null);
  const close = useRef(onClose);
  close.current = onClose;
  useEffect(() => {
    panel.current?.focus();
    const closeOutside = (event: MouseEvent) => {
      const target = event.target as Node;
      if (!panel.current?.contains(target) && !anchor?.current?.contains(target)) close.current(false);
    };
    document.addEventListener("mousedown", closeOutside);
    return () => document.removeEventListener("mousedown", closeOutside);
  }, [anchor]);
  return (
    <div
      ref={panel}
      className={styles.popover}
      role="dialog"
      aria-label={label}
      tabIndex={-1}
      onClick={(event) => event.stopPropagation()}
      onKeyDown={(event) => {
        if (event.key === "Escape") {
          event.stopPropagation();
          onClose(true);
        }
      }}
    >
      {children}
    </div>
  );
}
