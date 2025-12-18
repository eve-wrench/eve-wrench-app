# EVE Wrench

<p align="center">
  <img src="src-tauri/icons/128x128.png" width="128" height="128" alt="EVE Wrench Logo">
</p>

<p align="center">
  <strong>EVE Online Settings Manager</strong><br>
  Backup, restore, and sync settings across accounts and characters
</p>

---

## Features

- **Backup Settings** — Create named backups of account and character settings
- **Restore Backups** — Apply backups to any compatible account or character
- **Sync Settings** — Copy settings from one character to multiple targets
- **Cross-Server** — Works with Tranquility, Singularity, Thunderdome, and Serenity
- **Character Info** — Fetches character names and portraits from ESI (TQ only)
- **Account Aliases** — Add custom names to accounts for easy identification

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

1. **Launch EVE Wrench** — The app automatically detects your EVE installations
2. **Select a source** — Click the upload icon on any account or character to set it as the source
3. **Add targets** — Click the download icon on compatible entries to add them as targets
4. **Copy settings** — Click "Copy to X targets" to sync settings
5. **Backups** — Use the ⋯ menu on any entry to create or restore backups

### Backups

Backups are stored in the EVE settings directory under each profile's `backups` folder:

```
~/Library/Application Support/CCP/EVE/c_ccp_.../settings_Default/backups/
```

## Development

### Prerequisites

- [Node.js](https://nodejs.org/) 18+
- [Rust](https://rustup.rs/) 1.77+
- [Tauri CLI](https://tauri.app/start/prerequisites/)

### Setup

```bash
# Clone the repository
git clone https://github.com/YOUR_USERNAME/eve-wrench.git
cd eve-wrench

# Install dependencies
npm install

# Run in development mode
npm run tauri dev

# Build for production
npm run tauri build
```

### Scripts

```bash
npm run dev          # Start Vite dev server
npm run build        # Type-check and build
npm run tauri dev    # Run Tauri in development
npm run tauri build  # Build production app
npm run format       # Format code with Prettier
npm run format:check # Check formatting
npm run lint         # Lint with ESLint
```

### Project Structure

```
eve-wrench/
├── src/                    # Vue frontend
│   ├── components/         # Vue components
│   │   ├── ui/             # shadcn-vue components
│   │   ├── SettingsBrowser.vue
│   │   ├── ProfileCard.vue
│   │   ├── CopyPanel.vue
│   │   └── ...
│   ├── composables/        # Vue composables
│   │   ├── useCopyManager.ts
│   │   ├── useConfirm.ts
│   │   └── usePrompt.ts
│   └── types.ts            # TypeScript types
├── src-tauri/              # Rust backend
│   ├── src/
│   │   ├── evesettings.rs  # Settings discovery & management
│   │   ├── esi.rs          # EVE ESI API client
│   │   └── lib.rs          # Tauri commands
│   └── icons/              # App icons
└── package.json
```

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
