import { type Dispatch, type SetStateAction, useState } from "react";
import { commands, type Id, type Project } from "../bindings";
import { type VariableDraft, variableDrafts, variableEdits } from "../editors";
import { closeSheet } from "../ui";
import { ChipList, ChipToggle, Field, FieldHint, FieldLabel, FormSheet } from "./Form";
import { LayersIcon, PlusIcon, TrashIcon } from "./Icons";
import { VariableEditor } from "./VariableEditor";
import styles from "./EnvironmentForm.module.css";

/** One Environment Profile as edited here; `profileId` is `null` for one added here. */
type ProfileDraft = { id: number; profileId: Id | null; name: string; variables: VariableDraft[] };

let nextProfileId = 0;

/** Edits a Project's variables and its Environment Profiles, and which Profile is active. */
export function EnvironmentForm({ project }: { project: Project }) {
  const [variables, setVariables] = useState(() => variableDrafts(project.variables));
  const [profiles, setProfiles] = useState<ProfileDraft[]>(() =>
    (project.profiles ?? []).map((profile) => ({
      id: ++nextProfileId,
      profileId: profile.id,
      name: profile.name,
      variables: variableDrafts(profile.variables),
    })),
  );
  const [activeId, setActiveId] = useState<number | null>(
    () => profiles.find((draft) => draft.profileId !== null && draft.profileId === project.activeProfileID)?.id ?? null,
  );
  const [error, setError] = useState<string | null>(null);
  const [isSaving, setIsSaving] = useState(false);

  const suggestedName = () => {
    const names = new Set(profiles.map((profile) => profile.name));
    return (
      ["Development", "Testing", "Staging", "Production"].find((name) => !names.has(name)) ??
      `Profile ${profiles.length + 1}`
    );
  };

  const updateProfile = (id: number, change: Partial<ProfileDraft>) =>
    setProfiles((profiles) => profiles.map((profile) => (profile.id === id ? { ...profile, ...change } : profile)));

  /** A setter for one Profile's variables, for its VariableEditor. */
  const profileVariables =
    (id: number): Dispatch<SetStateAction<VariableDraft[]>> =>
    (action) =>
      setProfiles((profiles) =>
        profiles.map((profile) =>
          profile.id === id
            ? { ...profile, variables: typeof action === "function" ? action(profile.variables) : action }
            : profile,
        ),
      );

  const save = async () => {
    setIsSaving(true);
    setError(null);
    const active = profiles.findIndex((profile) => profile.id === activeId);
    const result = await commands.saveEnvironment(
      project.id,
      variableEdits(variables),
      profiles.map((profile) => ({
        profileId: profile.profileId,
        name: profile.name,
        variables: variableEdits(profile.variables),
      })),
      active < 0 ? null : active,
    );
    setIsSaving(false);
    if (result.status === "error") return setError(result.error.message);
    closeSheet();
  };

  return (
    <FormSheet
      icon={<LayersIcon size={16} />}
      title="Environment"
      subtitle={
        <>
          for <strong>{project.name}</strong>
        </>
      }
      hint="Running services show “restart to apply” when their environment changes."
      error={error}
      primaryTitle="Save"
      isPrimaryDisabled={isSaving}
      width={600}
      onCancel={closeSheet}
      onSubmit={() => void save()}
    >
      <Field title="Project variables" note="every service gets these" hint="A service’s own variables override them.">
        {() => (
          <VariableEditor
            variables={variables}
            setVariables={setVariables}
            layer={{ kind: "project", id: project.id }}
            label="Project variables"
          />
        )}
      </Field>
      <section className={styles.profiles} aria-label="Profiles">
        <div className={styles.profilesHeader}>
          <FieldLabel title="Profiles" />
          <button
            type="button"
            className="button compact"
            onClick={() =>
              setProfiles([...profiles, { id: ++nextProfileId, profileId: null, name: suggestedName(), variables: [] }])
            }
          >
            <PlusIcon size={10} />
            Add Profile
          </button>
        </div>
        {profiles.length === 0 ? (
          <FieldHint>
            A profile is a named set of variables, like “Testing”, that overrides the project’s and each service’s while
            it is active.
          </FieldHint>
        ) : (
          <>
            <Field title="Active profile">
              {() => (
                <ChipList label="Active profile">
                  <ChipToggle
                    title="None"
                    color="var(--text-secondary)"
                    isOn={activeId === null}
                    onChange={() => setActiveId(null)}
                  />
                  {profiles.map((profile) => (
                    <ChipToggle
                      key={profile.id}
                      title={profile.name || "Untitled"}
                      color="var(--accent)"
                      isOn={activeId === profile.id}
                      onChange={() => setActiveId(profile.id)}
                    />
                  ))}
                </ChipList>
              )}
            </Field>
            {profiles.map((profile) => (
              <div key={profile.id} className={styles.profile}>
                <div className={styles.profileHeader}>
                  <input
                    className={styles.profileName}
                    value={profile.name}
                    placeholder="Testing"
                    aria-label="Profile name"
                    spellCheck={false}
                    autoComplete="off"
                    onChange={(event) => updateProfile(profile.id, { name: event.target.value })}
                  />
                  {activeId === profile.id && <span className={styles.active}>Active</span>}
                  <button
                    type="button"
                    className="iconButton"
                    aria-label={`Remove ${profile.name || "this profile"}`}
                    title="Remove this profile"
                    onClick={() => {
                      setProfiles(profiles.filter((other) => other.id !== profile.id));
                      if (activeId === profile.id) setActiveId(null);
                    }}
                  >
                    <TrashIcon size={13} />
                  </button>
                </div>
                <VariableEditor
                  variables={profile.variables}
                  setVariables={profileVariables(profile.id)}
                  layer={profile.profileId ? { kind: "profile", id: profile.profileId } : null}
                  label={`Variables of ${profile.name || "the profile"}`}
                />
              </div>
            ))}
          </>
        )}
      </section>
    </FormSheet>
  );
}
