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
        ForEach($variables) { $variable in
            HStack {
                TextField("Name", text: $variable.name, prompt: Text("NAME"))
                    .labelsHidden()
                    .font(.body.monospaced())
                valueField($variable)
                if variable.isSecret {
                    Button(variable.isRevealed ? "Hide Value" : "Reveal Value",
                           systemImage: variable.isRevealed ? "eye.slash" : "eye") {
                        Task { await toggleReveal($variable) }
                    }
                    .help(variable.isRevealed ? "Hide the value" : "Reveal the value")
                }
                Button(variable.isSecret ? "Make Plain" : "Make Secret",
                       systemImage: variable.isSecret ? "lock.fill" : "lock.open") {
                    Task { await toggleSecret($variable) }
                }
                .help(variable.isSecret
                      ? "Secret: stored in your Keychain, never in the project file. Click to make it a plain variable."
                      : "Make it a secret")
                Button("Remove Variable", systemImage: "minus.circle") {
                    variables.removeAll { $0.id == variable.id }
                }
                .help("Remove")
            }
            .labelStyle(.iconOnly)
            .buttonStyle(.borderless)
        }
        Button("Add Variable", systemImage: "plus") { variables.append(VariableDraft()) }
            .buttonStyle(.borderless)
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
        .font(.body.monospaced())
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
