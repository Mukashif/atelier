# Releasing Atelier

Atelier updates itself from GitHub releases on `Mukashif/atelier`. Every
installer is signed with the **Atelier updater key**; the app only installs
updates whose signature matches the public key built into it.

## One-time setup

1. **Create the signing key** (pick a password and keep it somewhere safe —
   a password manager is ideal):

   ```bash
   npx tauri signer generate -w "$USERPROFILE/.tauri/atelier.key"
   ```

   This writes `atelier.key` (private — never commit or share it) and
   `atelier.key.pub` (public) to `%USERPROFILE%\.tauri\`.
   **Back up `atelier.key` and its password.** If you lose either, installed
   copies can't be updated any more and you'd have to reinstall by hand.

2. **Put the public key into the app**, then commit:

   ```bash
   scripts/set-pubkey.sh
   ```

3. **Tell the release script where the key is** — create `.env.release`
   in the repo root (it's gitignored):

   ```bash
   TAURI_SIGNING_PRIVATE_KEY_PATH=C:/Users/<you>/.tauri/atelier.key
   TAURI_SIGNING_PRIVATE_KEY_PASSWORD=<your password>
   ```

4. Make sure `gh auth status` shows you're signed in to GitHub.

## Each release

```bash
# Try it first: builds the signed installer and latest.json, publishes nothing.
ATELIER_DRY_RUN=1 scripts/release.sh 0.2.0 "Faster first scan; cover size switch"

# For real: bumps versions, tests, builds, commits, tags, pushes and creates
# the GitHub release with the installer, its .sig and latest.json.
scripts/release.sh 0.2.0 "Faster first scan; cover size switch"
```

Run it from Git Bash on `main` with a clean working tree.

## How updates reach the app

- The app checks
  `https://github.com/Mukashif/atelier/releases/latest/download/latest.json`
  on launch (if "Check for updates on launch" is on) and from
  Settings → Updates → Check now.
- `latest.json` points at the installer on the same release and carries its
  signature. If the signature doesn't match the built-in public key, the
  update is refused.
- When an update is found, an "Update ready" chip appears in the top bar;
  clicking it shows what's new with **Install and restart** / **Later**.
- Windows installs in passive mode (a small progress window, no questions)
  and Atelier reopens afterwards.
