import { useEffect, useId, useState } from "react";
import { commands, type Settings } from "../bindings";
import { showAlert } from "../ui";
import { Dialog } from "./Dialog";
import styles from "./SettingsDialog.module.css";

type Props = {
  /** Set when Open in Editor needs an editor first: saving then opens it. */
  forEditor?: () => void;
  onClose: () => void;
};

/** The editor command and automatic port moves. */
export function SettingsDialog({ forEditor, onClose }: Props) {
  const [settings, setSettings] = useState<Settings | null>(null);
  const editorId = useId();
  const editorHintId = useId();
  const moveHintId = useId();

  useEffect(() => {
    void commands.settings().then(setSettings);
  }, []);

  const save = async () => {
    if (!settings) return;
    const result = await commands.saveSettings(settings);
    onClose();
    if (result.status === "error") showAlert({ kind: "error", message: result.error.message });
    else if (forEditor && settings.editorCommand?.trim()) forEditor();
  };

  return (
    <Dialog
      role="dialog"
      title={forEditor ? "Choose Editor" : "Settings"}
      onCancel={onClose}
      actions={
        <>
          <button className="button" onClick={onClose}>
            Cancel
          </button>
          <button
            className="button primary"
            onClick={() => void save()}
            disabled={!settings || (forEditor !== undefined && !settings.editorCommand?.trim())}
          >
            {forEditor ? "Open" : "Save"}
          </button>
        </>
      }
    >
      {settings && (
        <form
          className={styles.form}
          onSubmit={(event) => {
            event.preventDefault();
            void save();
          }}
        >
          <label htmlFor={editorId}>Editor command</label>
          <input
            id={editorId}
            className="mono"
            value={settings.editorCommand ?? ""}
            placeholder="code"
            spellCheck={false}
            autoComplete="off"
            aria-describedby={editorHintId}
            // The settings load after the dialog opens, so its own focus can't reach this.
            autoFocus
            onChange={(event) => setSettings({ ...settings, editorCommand: event.target.value })}
          />
          <p id={editorHintId} className={styles.hint}>
            Open in Editor runs it with the folder, in your login shell, e.g. <span className="mono">code</span>,{" "}
            <span className="mono">zed</span> or <span className="mono">subl</span>.
          </p>
          {!forEditor && (
            <>
              <label className={styles.check}>
                <input
                  type="checkbox"
                  checked={settings.automaticPortMove}
                  aria-describedby={moveHintId}
                  onChange={(event) => setSettings({ ...settings, automaticPortMove: event.target.checked })}
                />
                Move a service to a free port when its port is taken
              </label>
              <p id={moveHintId} className={styles.hint}>
                Only for services whose port can change and that no other service depends on. The move lasts for that
                run; the configured port is kept.
              </p>
            </>
          )}
          {/* Enter in the field submits. */}
          <button type="submit" hidden />
        </form>
      )}
    </Dialog>
  );
}
