import { useEffect, useState } from "react";
import { commands, type Project, type Service, type ServiceRelations } from "../bindings";
import { serviceVariables, type VariableDraft, variableDrafts, variableEdits } from "../editors";
import { tildePath } from "../paths";
import { formatList } from "../status";
import { portVariable, projectFolder } from "../targets";
import { closeSheet } from "../ui";
import {
  ChipList,
  ChipToggle,
  CommandField,
  Field,
  FieldGroup,
  FieldHint,
  FieldRow,
  FormSheet,
  SwitchRow,
  TextField,
  useFollowingKey,
} from "./Form";
import { TerminalIcon } from "./Icons";
import { serviceColor } from "./ServicesColumn";
import { VariableEditor } from "./VariableEditor";

type Props = {
  project: Project;
  /** The Service to edit; a new one when absent. */
  editing?: Service;
  onAdded?: (service: Service) => void;
};

/**
 * Adds or edits a Service: name, Key, working directory relative to the
 * Project, command line, Port and URL, environment variables, Dependencies
 * and whether Start All includes it.
 */
export function ServiceForm({ project, editing, onAdded }: Props) {
  const [name, setName] = useState(editing?.name ?? "");
  const key = useFollowingKey(editing?.key ?? "", editing !== undefined, async (name) => {
    const result = await commands.suggestedServiceKey(project.id, name, editing?.id ?? null);
    return result.status === "ok" ? result.data : null;
  });
  const [workingDirectory, setWorkingDirectory] = useState(editing?.workingDirectory ?? ".");
  const [commandLine, setCommandLine] = useState(editing?.commandLine ?? "");
  const [port, setPort] = useState(editing?.port != null ? String(editing.port) : "");
  const [url, setUrl] = useState(editing?.url ?? "");
  const [portCanChange, setPortCanChange] = useState(editing?.portCanChange ?? true);
  const [variables, setVariables] = useState<VariableDraft[]>(() => (editing ? variableDrafts(serviceVariables(editing)) : []));
  const [dependencies, setDependencies] = useState<string[]>(editing?.dependencies ?? []);
  const [isManual, setIsManual] = useState(editing?.isExcludedFromStartAll ?? false);
  const [relations, setRelations] = useState<ServiceRelations>({ cycles: [], dependents: [] });
  const [usesPort, setUsesPort] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [isSaving, setIsSaving] = useState(false);

  useEffect(() => {
    if (!editing) return;
    void commands.serviceRelations(editing.id).then((result) => {
      if (result.status === "ok") setRelations(result.data);
    });
  }, [editing]);

  useEffect(() => {
    let isCurrent = true;
    void commands.commandLineUsesPort(commandLine).then((uses) => isCurrent && setUsesPort(uses));
    return () => {
      isCurrent = false;
    };
  }, [commandLine]);

  const index = editing ? project.services.findIndex((service) => service.id === editing.id) : project.services.length;
  const others = project.services.filter((service) => service.id !== editing?.id);
  const trimmedPort = port.trim();

  const portFooter = (() => {
    if (!trimmedPort) return "Optional. With a port, the service counts as running once the port accepts connections.";
    const variable = portVariable(projectFolder(project).target);
    let footer = `The command gets the port as ${variable}, e.g. “next dev -p ${variable}”. It counts as running once the port accepts connections.`;
    if (variables.some((variable) => variable.name === "PORT")) footer += " The PORT variable below is replaced by this port.";
    if (portCanChange && !usesPort) {
      footer += ` The command doesn’t mention ${variable}: if it ignores PORT, turn off “Port can change”.`;
    }
    return footer;
  })();

  const manualFooter = (() => {
    if (!isManual) return "Start All skips it; start it on its own.";
    const dependents = relations.dependents.map((name) => `“${name}”`);
    if (dependents.length === 0) return "Start All skips this service.";
    const verb = dependents.length === 1 ? "depends" : "depend";
    return `Start All skips this service, but still starts it for ${formatList(dependents)}, which ${verb} on it.`;
  })();

  const changeName = (value: string) => {
    setName(value);
    key.follow(value);
  };

  const chooseDirectory = async () => {
    const chosen = await commands.chooseWorkingDirectory(projectFolder(project));
    if (chosen !== null) setWorkingDirectory(chosen);
  };

  const save = async () => {
    setIsSaving(true);
    setError(null);
    const service = {
      name,
      key: key.key,
      workingDirectory,
      commandLine,
      port,
      url,
      portCanChange,
      variables: variableEdits(variables),
      dependencies,
      isExcludedFromStartAll: isManual,
    };
    if (editing) {
      const result = await commands.editService(editing.id, service);
      setIsSaving(false);
      if (result.status === "error") return setError(result.error.message);
      closeSheet();
    } else {
      const result = await commands.addService(project.id, service);
      setIsSaving(false);
      if (result.status === "error") return setError(result.error.message);
      closeSheet();
      onAdded?.(result.data);
    }
  };

  const hasCycleHint = relations.cycles.length > 0;
  return (
    <FormSheet
      icon={<TerminalIcon size={16} />}
      tint={serviceColor(index)}
      title={editing ? `Edit ${editing.name}` : "New Service"}
      subtitle={
        <>
          in <strong>{project.name}</strong> · <span className="mono">{tildePath(project.path)}</span>
        </>
      }
      hint={`↩ to ${editing ? "save" : "add"}`}
      error={error}
      primaryTitle={editing ? "Save" : "Add Service"}
      isPrimaryDisabled={!name.trim() || !key.key.trim() || !commandLine.trim() || isSaving}
      width={580}
      onCancel={closeSheet}
      onSubmit={() => void save()}
    >
      <FieldRow>
        <Field title="Name">
          {(id) => <TextField id={id} value={name} onChange={changeName} placeholder="Web" autoFocus />}
        </Field>
        <Field title="Key" note="in logs & CLI" width={160}>
          {(id) => <TextField id={id} value={key.key} onChange={key.edit} placeholder="web" isMonospaced />}
        </Field>
      </FieldRow>
      <Field title="Command" note="runs in your login shell">
        {(id) => (
          <CommandField
            id={id}
            workingDirectory={workingDirectory}
            commandLine={commandLine}
            onCommandLineChange={setCommandLine}
            onChooseDirectory={() => void chooseDirectory()}
          />
        )}
      </Field>
      <FieldGroup>
        <FieldRow>
          <Field title="Port" width={120}>
            {(id) => <TextField id={id} value={port} onChange={setPort} placeholder="3000" isMonospaced />}
          </Field>
          <Field title="URL" note="optional">
            {(id) => (
              <TextField
                id={id}
                value={url}
                onChange={setUrl}
                placeholder={`http://localhost:${trimmedPort || "3000"}`}
                isMonospaced
              />
            )}
          </Field>
        </FieldRow>
        <FieldHint>{portFooter}</FieldHint>
        {trimmedPort && (
          <SwitchRow
            title="Port can change"
            detail="If the port is in use, this run may start on the next free one."
            isOn={portCanChange}
            onChange={setPortCanChange}
          />
        )}
      </FieldGroup>
      <Field title="Environment" note="overrides the project’s · applies on next start">
        {() => (
          <VariableEditor
            variables={variables}
            setVariables={setVariables}
            layer={editing ? { kind: "service", id: editing.id } : null}
            label="Service variables"
          />
        )}
      </Field>
      {others.length > 0 && (
        <Field
          title="Starts after"
          note="waits until they’re running"
          hint={hasCycleHint ? "Services that already depend on this one can’t be chosen: that would be a cycle." : null}
        >
          {() => (
            <ChipList label="Starts after">
              {others.map((other) => {
                const cycle = relations.cycles.find((cycle) => cycle.serviceId === other.id);
                const isOn = dependencies.includes(other.id);
                return (
                  <ChipToggle
                    key={other.id}
                    title={other.name}
                    color={serviceColor(project.services.indexOf(other))}
                    isOn={isOn}
                    isDisabled={cycle !== undefined}
                    help={cycle?.message ?? other.key}
                    onChange={(isOn) =>
                      setDependencies(isOn ? [...dependencies, other.id] : dependencies.filter((id) => id !== other.id))
                    }
                  />
                );
              })}
            </ChipList>
          )}
        </Field>
      )}
      <SwitchRow title="Manual" detail={manualFooter} isOn={isManual} onChange={setIsManual} />
    </FormSheet>
  );
}
