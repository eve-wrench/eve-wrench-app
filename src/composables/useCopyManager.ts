import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { toast } from 'vue-sonner'
import type {
    SourceItem,
    SettingsEntry,
    SettingsKind,
    ProfileData,
    BackupEntry,
    AppData,
} from '@/types'
import { isBackup } from '@/types'
import { useConfirm } from './useConfirm'
import { usePrompt } from './usePrompt'

const appData = ref<AppData | null>(null)
const loading = ref(true)
const copying = ref(false)
const source = ref<SourceItem | null>(null)
const targets = ref<SettingsEntry[]>([])
let listenerSetup = false

async function setupListener(loadDataFn: () => Promise<void>) {
    if (listenerSetup) return
    listenerSetup = true
    await listen('data-changed', () => {
        loadDataFn()
    })
}

export function useCopyManager() {
    const { confirm } = useConfirm()
    const { prompt } = usePrompt()

    const sourceKind = computed<SettingsKind | null>(() => {
        if (!source.value) return null
        return source.value.kind
    })

    const canCopy = computed(() => {
        return (
            source.value !== null && targets.value.length > 0 && !copying.value
        )
    })

    const hasData = computed(() => {
        return (
            appData.value &&
            (appData.value.servers.length > 0 ||
                appData.value.backups.length > 0)
        )
    })

    async function loadData(showToast = false) {
        loading.value = true
        try {
            appData.value = await invoke<AppData>('get_app_data')
            if (showToast) {
                const serverCount = appData.value?.servers.length || 0
                const backupCount = appData.value?.backups.length || 0
                if (serverCount > 0 || backupCount > 0) {
                    toast.success('Data refreshed', {
                        description: `Found ${serverCount} server(s) and ${backupCount} backup(s)`,
                    })
                }
            }
        } catch (e: unknown) {
            toast.error('Failed to load data', { description: String(e) })
        } finally {
            loading.value = false
        }
    }

    async function init() {
        await setupListener(loadData)
        await loadData()
    }

    function setSource(item: SourceItem) {
        const newKind = item.kind
        if (source.value && sourceKind.value !== newKind) {
            targets.value = []
        }
        source.value = item
        targets.value = targets.value.filter((t) => t.kind === newKind)
    }

    function clearSource() {
        source.value = null
    }

    function addTarget(entry: SettingsEntry) {
        if (!source.value) {
            toast.error('No source selected', {
                description: 'Select a source first before adding targets',
            })
            return
        }
        if (entry.kind !== sourceKind.value) {
            toast.error('Type mismatch', {
                description: 'Target must be the same type as source',
            })
            return
        }
        if (!isBackup(source.value) && source.value.path === entry.path) {
            toast.error('Invalid target', {
                description: 'Cannot use the same file as source and target',
            })
            return
        }
        if (targets.value.some((t) => t.path === entry.path)) {
            return
        }
        targets.value.push(entry)
    }

    function removeTarget(entry: SettingsEntry) {
        targets.value = targets.value.filter((t) => t.path !== entry.path)
    }

    function clearTargets() {
        targets.value = []
    }

    function addAllFromProfile(profile: ProfileData, kind: SettingsKind) {
        if (!source.value) {
            toast.error('No source selected', {
                description: 'Select a source first before adding targets',
            })
            return
        }
        if (sourceKind.value !== kind) return

        const items = kind === 'char' ? profile.characters : profile.accounts
        for (const item of items) {
            if (!isBackup(source.value) && source.value.path === item.path)
                continue
            if (targets.value.some((t) => t.path === item.path)) continue
            targets.value.push(item)
        }
    }

    async function executeCopy() {
        if (!source.value || targets.value.length === 0) return

        const confirmed = await confirm({
            title: 'Copy Settings',
            description: `Copy settings from "${source.value.display_name}" to ${targets.value.length} target(s)?`,
            confirmText: 'Copy',
        })
        if (!confirmed) return

        copying.value = true
        try {
            const sourcePath = source.value.path
            const targetPaths = targets.value.map((t) => t.path)
            const count = await invoke<number>('copy_settings', {
                sourcePath,
                targetPaths,
            })
            toast.success('Settings copied', {
                description: `Successfully copied to ${count} target(s)`,
            })
            targets.value = []
        } catch (e: unknown) {
            toast.error('Copy failed', { description: String(e) })
        } finally {
            copying.value = false
        }
    }

    async function createBackup(entry: SettingsEntry) {
        const name = await prompt({
            title: 'Create Backup',
            description: `Enter a name for this backup of ${entry.display_name}`,
            placeholder: 'Backup name',
            defaultValue: entry.display_name,
            confirmText: 'Create',
        })
        if (!name) return

        try {
            await invoke('create_backup', {
                sourcePath: entry.path,
                backupName: name,
            })
            toast.success('Backup created', {
                description: `"${name}" has been saved`,
            })
        } catch (e: unknown) {
            toast.error('Backup failed', { description: String(e) })
        }
    }

    async function deleteBackup(backup: BackupEntry) {
        const confirmed = await confirm({
            title: 'Delete Backup',
            description: `Are you sure you want to delete "${backup.name}"?`,
            confirmText: 'Delete',
            destructive: true,
        })
        if (!confirmed) return

        try {
            await invoke('delete_backup', { backupPath: backup.path })
            toast.success('Backup deleted', {
                description: `"${backup.name}" has been removed`,
            })
            if (
                source.value &&
                isBackup(source.value) &&
                source.value.id === backup.id
            ) {
                source.value = null
            }
        } catch (e: unknown) {
            toast.error('Delete failed', { description: String(e) })
        }
    }

    function getBackupsForEntry(entry: SettingsEntry): BackupEntry[] {
        if (!appData.value) return []
        return appData.value.backups.filter(
            (b) => b.kind === entry.kind && b.original_id === entry.id
        )
    }

    async function restoreBackup(entry: SettingsEntry, backup: BackupEntry) {
        const confirmed = await confirm({
            title: 'Restore Backup',
            description: `Restore "${backup.name}" to ${entry.display_name}? This will overwrite current settings.`,
            confirmText: 'Restore',
            destructive: true,
        })
        if (!confirmed) return

        try {
            await invoke<number>('copy_settings', {
                sourcePath: backup.path,
                targetPaths: [entry.path],
            })
            toast.success('Backup restored', {
                description: `"${backup.name}" has been applied`,
            })
        } catch (e: unknown) {
            toast.error('Restore failed', { description: String(e) })
        }
    }

    async function applyBackup(backup: BackupEntry, target: SettingsEntry) {
        const confirmed = await confirm({
            title: 'Apply Backup',
            description: `Apply "${backup.name}" to ${target.display_name}? This will overwrite current settings.`,
            confirmText: 'Apply',
            destructive: true,
        })
        if (!confirmed) return

        try {
            await invoke<number>('copy_settings', {
                sourcePath: backup.path,
                targetPaths: [target.path],
            })
            toast.success('Backup applied', {
                description: `"${backup.name}" has been applied to ${target.display_name}`,
            })
        } catch (e: unknown) {
            toast.error('Apply failed', { description: String(e) })
        }
    }

    function isSource(item: SourceItem): boolean {
        if (!source.value) return false
        if (isBackup(item) && isBackup(source.value)) {
            return source.value.id === item.id
        }
        if (!isBackup(item) && !isBackup(source.value)) {
            return source.value.path === item.path
        }
        return false
    }

    function isTarget(entry: SettingsEntry): boolean {
        return targets.value.some((t) => t.path === entry.path)
    }

    function refresh() {
        loadData(true)
    }

    async function setBracketsAlwaysShow(serverPath: string, enabled: boolean) {
        try {
            await invoke('set_brackets_always_show', { serverPath, enabled })
            toast.success('Setting updated', {
                description: `Brackets always show ${enabled ? 'enabled' : 'disabled'}`,
            })
        } catch (e: unknown) {
            toast.error('Failed to update setting', { description: String(e) })
        }
    }

    return {
        appData,
        loading,
        copying,
        source,
        targets,
        sourceKind,
        canCopy,
        hasData,
        init,
        refresh,
        setSource,
        clearSource,
        addTarget,
        removeTarget,
        clearTargets,
        addAllFromProfile,
        executeCopy,
        createBackup,
        deleteBackup,
        getBackupsForEntry,
        restoreBackup,
        applyBackup,
        isSource,
        isTarget,
        setBracketsAlwaysShow,
    }
}
