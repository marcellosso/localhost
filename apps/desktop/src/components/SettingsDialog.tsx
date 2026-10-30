import { type ReactNode, useId, useRef, useState } from "react";
import type { Appearance } from "../bindings";
import { saveSettings, useSettings } from "../settings";
import { checkForUpdates } from "../updates";
import { CONNECT_AGENTS_SUBTITLE, CONNECT_AGENTS_TITLE, ConnectAgents } from "./ConnectAgents";
import { Dialog, useModal } from "./Dialog";
import { CircleHalfIcon, GearIcon, GlobeIcon, LinkIcon } from "./Icons";
import styles from "./SettingsDialog.module.css";

type Section = "general" | "appearance" | "ports" | "agents";

const sections: { id: Section; title: string; icon: ReactNode }[] = [
  { id: "general", title: "General", icon: <GearIcon size={13} /> },
  { id: "appearance", title: "Appearance", icon: <CircleHalfIcon size={13} /> },
  { id: "ports", title: "Ports", icon: <GlobeIcon size={13} /> },
  { id: "agents", title: "Agents", icon: <LinkIcon size={13} /> },
];

/** The log font sizes offered; settings.rs keeps the same list. */
const LOG_FONT_SIZES = [10, 11, 12, 13, 14, 16];

/**
 * Settings: a rail of sections and the selected one beside it. Changes
 * apply at once.
 */
export function SettingsDialog({ onClose }: { onClose: () => void }) {
  const [section, setSection] = useState<Section>("appearance");
  const panel = useRef<HTMLDivElement>(null);
  const titleId = useId();
  // Starts on the current section; Tab stays inside, Escape closes.
  const onKeyDown = useModal(panel, onClose);

  return (
    <div className={styles.backdrop}>
      <div
        ref={panel}
        className={styles.window}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        onKeyDown={onKeyDown}
      >
        <h2 id={titleId} className={styles.hidden}>
          Settings
        </h2>
        <nav className={styles.rail} aria-label="Settings sections">
          {sections.map((item) => (
            <button
              key={item.id}
              className={styles.railItem}
              aria-current={section === item.id}
              data-autofocus={section === item.id ? true : undefined}
              onClick={() => setSection(item.id)}
            >
              {item.icon}
              {item.title}
            </button>
          ))}
        </nav>
        <div className={styles.content}>
          {section === "general" && <GeneralSettings />}
          {section === "appearance" && <AppearanceSettings />}
          {section === "ports" && <PortsSettings />}
          {section === "agents" && (
            <>
              <SectionHeader title={CONNECT_AGENTS_TITLE} subtitle={CONNECT_AGENTS_SUBTITLE} />
              <ConnectAgents headingLevel={4} />
            </>
          )}
        </div>
        <button className={`button ${styles.done}`} onClick={onClose}>
          Done
        </button>
      </div>
    </div>
  );
}

function SectionHeader({ title, subtitle }: { title: string; subtitle: string }) {
  return (
    <header className={styles.sectionHeader}>
      <h3>{title}</h3>
      <p>{subtitle}</p>
    </header>
  );
}

/** A setting: its name and a line of explanation, the control on the right. */
function SettingsRow({ title, detail, children }: { title: string; detail?: string; children: ReactNode }) {
  return (
    <div className={styles.row}>
      <div className={styles.rowText}>
        <span>{title}</span>
        {detail && <span className={styles.detail}>{detail}</span>}
      </div>
      {children}
    </div>
  );
}

function GeneralSettings() {
  const settings = useSettings();
  const [editor, setEditor] = useState(settings.editorCommand ?? "");
  const save = () => {
    if (editor.trim() !== (settings.editorCommand ?? "")) void saveSettings({ editorCommand: editor.trim() || null });
  };
  return (
    <>
      <SectionHeader title="General" subtitle="How localhost works with the rest of your desktop." />
      <SettingsRow
        title="Editor"
        detail="“Open in Editor” runs this command with the folder, in your login shell."
      >
        <input
          className={`${styles.field} mono`}
          aria-label="Editor command"
          placeholder="None"
          value={editor}
          spellCheck={false}
          autoComplete="off"
          onChange={(event) => setEditor(event.target.value)}
          onBlur={save}
          onKeyDown={(event) => event.key === "Enter" && save()}
        />
      </SettingsRow>
      <SettingsRow
        title="Check for updates automatically"
        detail="At launch and every few hours. A new version is offered, never installed without asking."
      >
        <Switch
          label="Check for updates automatically"
          isOn={settings.automaticUpdateChecks}
          onChange={(automaticUpdateChecks) => void saveSettings({ automaticUpdateChecks })}
        />
      </SettingsRow>
      <SettingsRow title="Check for updates now">
        <button className="button" onClick={() => void checkForUpdates()}>
          Check Now
        </button>
      </SettingsRow>
    </>
  );
}

function AppearanceSettings() {
  const settings = useSettings();
  return (
    <>
      <SectionHeader
        title="Appearance"
        subtitle="How localhost looks. Service colours and state glyphs adapt to both themes."
      />
      <p className={styles.caption}>THEME</p>
      <div className={styles.tiles} role="radiogroup" aria-label="Theme">
        {(["system", "light", "dark"] as Appearance[]).map((appearance) => (
          <ThemeTile
            key={appearance}
            appearance={appearance}
            isSelected={settings.appearance === appearance}
            onSelect={() => void saveSettings({ appearance })}
          />
        ))}
      </div>
      <SettingsRow title="Log font size" detail="The log and the commands in service rows.">
        <select
          className={styles.select}
          aria-label="Log font size"
          value={settings.logFontSize}
          onChange={(event) => void saveSettings({ logFontSize: Number(event.target.value) })}
        >
          {LOG_FONT_SIZES.map((size) => (
            <option key={size} value={size}>
              {size} pt
            </option>
          ))}
        </select>
      </SettingsRow>
    </>
  );
}

/** A small picture of the window in a theme; System is half light, half dark. */
function ThemeTile({ appearance, isSelected, onSelect }: { appearance: Appearance; isSelected: boolean; onSelect: () => void }) {
  const title = { system: "System", light: "Light", dark: "Dark" }[appearance];
  return (
    <button
      className={styles.tile}
      role="radio"
      aria-checked={isSelected}
      aria-label={`${title} theme`}
      onClick={onSelect}
    >
      <span className={styles.preview} aria-hidden="true">
        <WindowSketch dark={appearance === "dark"} />
        {appearance === "system" && (
          <span className={styles.darkHalf}>
            <WindowSketch dark />
          </span>
        )}
      </span>
      <span>{title}</span>
    </button>
  );
}

function WindowSketch({ dark }: { dark: boolean }) {
  return (
    <span className={`${styles.sketch} ${dark ? styles.sketchDark : ""}`}>
      <span />
      <span />
      <span>
        <i className={styles.accentLine} />
        <i />
        <i />
      </span>
    </span>
  );
}

function PortsSettings() {
  const settings = useSettings();
  return (
    <>
      <SectionHeader title="Ports" subtitle="What happens when a service’s port is already taken." />
      <SettingsRow
        title="Start on a free port when a service’s port is in use"
        detail="The service gets the next free port as its PORT variable for that run; its configured port is kept. Services others depend on, or whose port can’t change, are never moved."
      >
        <Switch
          label="Start on a free port"
          isOn={settings.automaticPortMove}
          onChange={(automaticPortMove) => void saveSettings({ automaticPortMove })}
        />
      </SettingsRow>
    </>
  );
}

function Switch({ label, isOn, onChange }: { label: string; isOn: boolean; onChange: (isOn: boolean) => void }) {
  return (
    <button className={styles.switch} role="switch" aria-checked={isOn} aria-label={label} onClick={() => onChange(!isOn)}>
      <span />
    </button>
  );
}

/** Asked the first time Open in Editor has no editor: saving opens the folder. */
export function ChooseEditorDialog({ onChosen, onClose }: { onChosen: () => void; onClose: () => void }) {
  const [editor, setEditor] = useState("");
  const fieldId = useId();
  const choose = async () => {
    if (!editor.trim()) return;
    onClose();
    if (await saveSettings({ editorCommand: editor.trim() })) onChosen();
  };
  return (
    <Dialog
      title="Choose Editor"
      onCancel={onClose}
      actions={
        <>
          <button className="button" onClick={onClose}>
            Cancel
          </button>
          <button className="button accent" disabled={!editor.trim()} onClick={() => void choose()}>
            Open
          </button>
        </>
      }
    >
      <p>localhost opens projects with this command, in your login shell.</p>
      <input
        id={fieldId}
        className={`${styles.field} ${styles.wide} mono`}
        aria-label="Editor command"
        placeholder="code"
        value={editor}
        spellCheck={false}
        autoComplete="off"
        data-autofocus
        onChange={(event) => setEditor(event.target.value)}
        onKeyDown={(event) => event.key === "Enter" && void choose()}
      />
    </Dialog>
  );
}
