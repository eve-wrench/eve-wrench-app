# EVE Wrench

<p align="center">
  <img src="src-tauri/icons/128x128.png" width="128" height="128" alt="EVE Wrench Logo">
</p>

<p align="center">
  <strong>EVE Online Settings Manager</strong><br>
  Backup, restore, and sync settings across accounts and characters
</p>

---

<img width="1112" height="712" alt="app" src="https://github.com/user-attachments/assets/8ed5f561-79ad-4162-bea2-e1461e152975" />

## Features

### Settings Sync

Copy settings from one character or account to multiple targets with a single click. Select a source, add one or more targets, then hit copy. Perfect for:

- Setting up new alts with your preferred overview, window layouts, and keybinds
- Applying a "master" account's settings across all your accounts
- Syncing characters across different settings profiles

The app enforces type compatibility—account settings can only be copied to other accounts, character settings to other characters.

### Backup & Restore

Create named backups before making changes. Each backup is timestamped and tied to its original account or character. Backups are stored directly in EVE's settings directory structure, making them easy to find and manage.

- **Create backups** from the ⋯ menu on any account or character
- **Restore backups** to the original entity or apply them to any compatible target
- **Manage backups** in the dedicated Backups tab—sort by name or time, delete old backups

### Cross-Server Support

Works with all EVE servers:

| Server          | Description             |
| --------------- | ----------------------- |
| **Tranquility** | Main production server  |
| **Singularity** | Public test server      |
| **Thunderdome** | Tournament/event server |
| **Serenity**    | Chinese server          |

Each server's settings are completely separate. The app detects all installed servers automatically.

### Character & Account Info

For Tranquility and Singularity characters, the app fetches data from EVE's ESI API:

- Character names and portraits
- Corporation names

Since Singularity is a mirror of Tranquility, most character IDs exist on both servers—so SISI characters get names and portraits too. This makes it easy to identify characters at a glance instead of seeing raw character IDs.

### Aliases

EVE doesn't store account names in settings files—you only see numeric account IDs. EVE Wrench lets you assign custom aliases to accounts so you can tell them apart.

For characters on servers where ESI doesn't work (Thunderdome, Serenity) or when a character can't be found via ESI, you can also set aliases to identify them. Aliases are stored locally and persist across sessions.

### Extra Settings

Some useful client settings aren't exposed in EVE's UI. EVE Wrench gives you access to:

- **Always Show Bracket Text** — Display ship labels on all brackets in space, not just selected targets. Useful for PvP situational awareness. May impact performance in large fights (200+ pilots). Requires a client restart to take effect.

### Auto-Update Notifications

The app checks for new versions on GitHub at startup. When an update is available, a modal shows the version change and release notes with a direct link to download.

---

## EVE Settings File Structure

EVE Online stores all user settings locally. Understanding this structure helps you manage settings manually or troubleshoot issues.

### Root Directory

| OS      | Path                                     |
| ------- | ---------------------------------------- |
| macOS   | `~/Library/Application Support/CCP/EVE/` |
| Windows | `%LocalAppData%\CCP\EVE\`                |

### Server Folders

Inside the root, you'll find folders for each server you've connected to:

```
CCP/EVE/
├── c_ccp_eve_online_tq_tranquility/
├── c_ccp_eve_online_sisi_singularity/
├── c_ccp_eve_online_tq_thunderdome/
└── c_ccp_eve_online_cn_serenity/
```

The folder names encode the server identity. EVE Wrench parses these automatically.

### Settings Files

Inside each profile folder, you'll find the actual settings files:

```
settings_Default/
├── core_user_12345678.dat      # Account settings
├── core_user_87654321.dat      # Another account
├── core_char_90000001.dat      # Character settings
├── core_char_90000002.dat      # Another character
├── prefs.ini                   # Client preferences
└── backups/                    # EVE Wrench backups
    ├── MyBackup_user_12345678_1703001234.bak
    └── BeforeChanges_char_90000001_1703002345.bak
```

#### Account Settings (`core_user_*.dat`)

Binary files containing account-level settings:

- Window positions and sizes
- Overview settings and tab configurations
- Audio settings
- Keyboard shortcuts
- UI scaling and font sizes
- Notification preferences

The numeric ID is your account ID (not character ID).

#### Character Settings (`core_char_*.dat`)

Binary files containing character-specific settings:

- Drone settings
- Autopilot preferences
- Channel settings
- D-scan presets
- Some UI state

The numeric ID is the character ID. Character IDs ≥ 90,000,000 are player characters; lower IDs are NPCs or legacy characters.

#### Client Preferences (`prefs.ini`)

A text file with client-wide settings like:

```ini
bracketsAlwaysShowShipText=1
```

Some settings in this file aren't exposed in EVE's UI but can significantly affect gameplay.

#### Backups Folder

EVE Wrench stores backups in a `backups/` subfolder within each profile. Backup filenames follow this pattern:

```
{name}_{type}_{id}_{timestamp}.bak
```

- **name**: The backup name you provided
- **type**: `user` (account) or `char` (character)
- **id**: The account or character ID
- **timestamp**: Unix timestamp when the backup was created

---

## Installation

### macOS

Download the latest `.dmg` from [Releases](https://github.com/YOUR_USERNAME/eve-wrench/releases), open it, and drag EVE Wrench to your Applications folder.

### Windows

Download the latest `.msi` installer from [Releases](https://github.com/YOUR_USERNAME/eve-wrench/releases) and run it.

### Linux

Download the latest `.AppImage` from [Releases](https://github.com/YOUR_USERNAME/eve-wrench/releases):

```bash
chmod +x eve-wrench_*.AppImage
./eve-wrench_*.AppImage
```

## Usage

1. **Launch EVE Wrench** — The app automatically scans for EVE installations and loads all accounts and characters
2. **Browse servers** — Switch between servers using the tabs at the top
3. **Set a source** — Click the upload icon (↑) on any account or character, or use the ⋯ menu
4. **Add targets** — Click the download icon (↓) on compatible entries to add them as copy targets
5. **Execute** — Click "Copy Settings" in the right panel to sync settings
6. **Backups** — Use the ⋯ menu to create backups before making changes, or browse all backups in the Backups tab

### Tips

- Sort accounts/characters by name or modification time by clicking column headers
- Use "Add all" to quickly select all accounts or characters in a profile as targets
- Assign aliases to accounts via the ⋯ menu to tell them apart
- Backups show which entity they came from, making it easy to find the right one

---

## Development

### Prerequisites

- [Node.js](https://nodejs.org/) 18+
- [Rust](https://rustup.rs/) 1.77+
- [Tauri CLI](https://tauri.app/start/prerequisites/)

### Setup

```bash
git clone https://github.com/YOUR_USERNAME/eve-wrench.git
cd eve-wrench

npm install

npm run tauri dev

npm run tauri build
```

### Scripts

| Command                | Description               |
| ---------------------- | ------------------------- |
| `npm run dev`          | Start Vite dev server     |
| `npm run build`        | Type-check and build      |
| `npm run tauri dev`    | Run Tauri in development  |
| `npm run tauri build`  | Build production app      |
| `npm run format`       | Format code with Prettier |
| `npm run format:check` | Check formatting          |
| `npm run lint`         | Lint with ESLint          |

## Tech Stack

- **Frontend**: Vue 3, TypeScript, Tailwind CSS, shadcn-vue
- **Backend**: Rust, Tauri 2
- **APIs**: EVE ESI (character info)
- **Icons**: Lucide

## License

MIT

---

<p align="center">
  <sub>Not affiliated with CCP Games. EVE Online and all related logos are trademarks of CCP hf.</sub>
</p>
