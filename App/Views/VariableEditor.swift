import LocalhostCore
import SwiftUI

/// One variable row being edited. A Secret loaded from the configuration
/// has no value here until it is revealed or typed again.
struct VariableDraft: Identifiable {
    let id = UUID()
    var name = ""
    var value = ""
    var isSecret = false
    /// The name the Secret is stored under, for a Secret loaded from the configuration.
    var storedSecretName: String?
    /// Whether `value` holds the Secret's value (typed or revealed).
    var isValueKnown = true
    var isRevealed = false

    /// Plain variables first, then Secrets, each sorted by name.
    static func drafts(from variables: Variables) -> [VariableDraft] {
        variables.values.sorted { $0.key < $1.key }.map { VariableDraft(name: $0.key, value: $0.value) }
            + variables.secretNames.map { VariableDraft(name: $0, isSecret: true, storedSecretName: $0, isValueKnown: false) }
    }

    /// What to save, or `nil` for an empty row.
    var input: VariableInput? {
        guard !name.isEmpty || !value.isEmpty else { return nil }
        if !isSecret { return .plain(name, value) }
        if let storedSecretName, !isValueKnown { return .existingSecret(name, previousName: storedSecretName) }
        return .secret(name, value)
    }
}

/// Edits one layer's variables. Secrets are masked; the eye reveals one,
/// the lock turns a variable into a Secret or back.
struct VariableEditor: View {
    @Environment(AppModel.self) private var model
    @Binding var variables: [VariableDraft]
    /// Where stored Secrets are read from; `nil` while the layer is new.
    let layer: VariableLayer?

    var body: some View {
        FormCard {
            ForEach(variables) { draft in
                let binding = $variables.element(draft)
                let variable = binding.wrappedValue
                HStack(spacing: 8) {
                    TextField("Name", text: binding.name, prompt: Text("NAME"))
                        .labelsHidden()
                        .textFieldStyle(.plain)
                        .font(.system(size: 12.5, design: .monospaced))
                        .frame(width: 150)
                    valueField(binding)
                    if variable.isSecret {
                        Button(variable.isRevealed ? "Hide Value" : "Reveal Value",
                               systemImage: variable.isRevealed ? "eye.slash" : "eye") {
                            Task { await toggleReveal(binding) }
                        }
                        .help(variable.isRevealed ? "Hide the value" : "Reveal the value")
                    }
                    Button(variable.isSecret ? "Make Plain" : "Make Secret",
                           systemImage: variable.isSecret ? "lock.fill" : "lock.open") {
                        Task { await toggleSecret(binding) }
                    }
                    .foregroundStyle(variable.isSecret ? AnyShapeStyle(Theme.accentText) : AnyShapeStyle(.secondary))
                    .help(variable.isSecret
                          ? "Secret: stored in your Keychain, never in the project file. Click to make it a plain variable."
                          : "Make it a secret")
                    Button("Remove Variable", systemImage: "minus.circle") {
                        variables.removeAll { $0.id == variable.id }
                    }
                    .foregroundStyle(.secondary)
                    .help("Remove")
                }
                .labelStyle(.iconOnly)
                .buttonStyle(.borderless)
                .padding(.horizontal, 10)
                .padding(.vertical, 7)
                .overlay(alignment: .bottom) { Divider().opacity(0.6) }
            }
            Button { variables.append(VariableDraft()) } label: {
                HStack(spacing: 6) {
                    Image(systemName: "plus").font(.system(size: 10, weight: .semibold))
                    Text("Add variable")
                }
                .font(.caption)
                .foregroundStyle(.secondary)
                .padding(.horizontal, 10)
                .padding(.vertical, 8)
                .frame(maxWidth: .infinity, alignment: .leading)
                .contentShape(.rect)
            }
            .buttonStyle(.plain)
        }
    }

    @ViewBuilder
    private func valueField(_ variable: Binding<VariableDraft>) -> some View {
        let text = Binding {
            variable.wrappedValue.value
        } set: { newValue in
            variable.wrappedValue.value = newValue
            variable.wrappedValue.isValueKnown = true
        }
        let prompt = Text(variable.wrappedValue.isValueKnown ? "value" : "Saved — type to replace")
        Group {
            if variable.wrappedValue.isSecret && !variable.wrappedValue.isRevealed {
                SecureField("Value", text: text, prompt: prompt)
            } else {
                TextField("Value", text: text, prompt: prompt)
            }
        }
        .labelsHidden()
        .textFieldStyle(.plain)
        .font(.system(size: 12.5, design: .monospaced))
        .foregroundStyle(.secondary)
    }

    private func toggleReveal(_ variable: Binding<VariableDraft>) async {
        if !variable.wrappedValue.isRevealed { await loadStoredValue(variable) }
        variable.wrappedValue.isRevealed.toggle()
    }

    /// Making a stored Secret plain needs its value first.
    private func toggleSecret(_ variable: Binding<VariableDraft>) async {
        if variable.wrappedValue.isSecret { await loadStoredValue(variable) }
        variable.wrappedValue.isSecret.toggle()
        variable.wrappedValue.isRevealed = false
    }

    private func loadStoredValue(_ variable: Binding<VariableDraft>) async {
        guard !variable.wrappedValue.isValueKnown, let layer,
              let name = variable.wrappedValue.storedSecretName
        else { return }
        variable.wrappedValue.value = await model.secretValue(named: name, in: layer) ?? ""
        variable.wrappedValue.isValueKnown = true
    }
}

extension Binding {
    /// A binding to the element with `element`'s id. Unlike `ForEach($array)`'s
    /// index-based bindings, it survives the element being removed: reads fall
    /// back to the last known value and writes are dropped.
    func element<Element: Identifiable>(_ element: Element) -> Binding<Element> where Value == [Element] {
        Binding<Element> {
            wrappedValue.first { $0.id == element.id } ?? element
        } set: { newValue in
            guard let index = wrappedValue.firstIndex(where: { $0.id == element.id }) else { return }
            wrappedValue[index] = newValue
        }
    }
}
