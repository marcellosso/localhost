**Edit Environment…** (in the Project's menu) holds the Project's variables and its **Environment Profiles**; each Service's own variables are in its editor.

## Layers

A Service gets, later ones winning:

1. your login shell's environment (on Windows, your user environment as a new sign-in would see it),
2. the Project's variables,
3. the Service's variables,
4. the active Environment Profile's variables (picked in the Project's toolbar),

plus `PORT` when it has a Port, and `FORCE_COLOR=1`, `CLICOLOR_FORCE=1`, `PYTHONUNBUFFERED=1` so tools keep their colours and don't buffer output.

## Secrets

Click the lock next to a variable to make it a **Secret**: its value goes to your system's secret store and stays hidden until you click the eye. Secret values are never written to configuration files, logs or Project files.

| | Secret store |
|---|---|
| macOS | login Keychain (see [macOS](/docs/macos)) |
| Linux | the Secret Service: GNOME Keyring, KWallet, KeePassXC (see [Linux](/docs/linux)) |
| Windows | Credential Manager |

Changes apply the next time a Service starts; a running Service shows “Restart to apply”.
