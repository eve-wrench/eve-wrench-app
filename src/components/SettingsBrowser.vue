<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import { Archive, ChevronUp, ChevronDown } from 'lucide-vue-next'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import {
    Table,
    TableBody,
    TableHead,
    TableHeader,
    TableRow,
} from '@/components/ui/table'
import ServerSection from './ServerSection.vue'
import BackupItem from './BackupItem.vue'
import type {
    AppData,
    ProfileData,
    SettingsEntry,
    SettingsKind,
    BackupEntry,
} from '@/types'
import { getServerColor } from '@/types'

const props = defineProps<{
    appData: AppData
    sourceKind: SettingsKind | null
    isSource: (entry: SettingsEntry) => boolean
    isTarget: (entry: SettingsEntry) => boolean
    isBackupSource: (backup: BackupEntry) => boolean
}>()

const emit = defineEmits<{
    setSource: [entry: SettingsEntry]
    addTarget: [entry: SettingsEntry]
    backup: [entry: SettingsEntry]
    restore: [entry: SettingsEntry, backup: BackupEntry]
    applyBackup: [backup: BackupEntry, target: SettingsEntry]
    addAllFromProfile: [profile: ProfileData, kind: SettingsKind]
    setBackupSource: [backup: BackupEntry]
    deleteBackup: [backup: BackupEntry]
    refresh: []
    setBracketsAlwaysShow: [serverPath: string, enabled: boolean]
}>()

const activeTab = ref(props.appData.servers[0]?.info.id || '')

watch(
    () => props.appData.servers,
    (servers) => {
        if (
            servers.length &&
            !servers.find((s) => s.info.id === activeTab.value)
        ) {
            activeTab.value = servers[0].info.id
        }
    },
    { immediate: true }
)

type SortColumn = 'name' | 'time'
type SortDirection = 'asc' | 'desc'

const backupSortCol = ref<SortColumn>('time')
const backupSortDir = ref<SortDirection>('desc')

function toggleBackupSort(col: SortColumn) {
    if (backupSortCol.value === col) {
        backupSortDir.value = backupSortDir.value === 'asc' ? 'desc' : 'asc'
    } else {
        backupSortCol.value = col
        backupSortDir.value = col === 'time' ? 'desc' : 'asc'
    }
}

const sortedBackups = computed(() => {
    return [...props.appData.backups].sort((a, b) => {
        let cmp = 0
        if (backupSortCol.value === 'name') {
            cmp = a.name.localeCompare(b.name)
        } else {
            cmp = a.timestamp - b.timestamp
        }
        return backupSortDir.value === 'asc' ? cmp : -cmp
    })
})

function getTargetsForBackup(backup: BackupEntry): SettingsEntry[] {
    const entries: SettingsEntry[] = []
    for (const server of props.appData.servers) {
        for (const profile of server.profiles) {
            const items =
                backup.kind === 'char' ? profile.characters : profile.accounts
            entries.push(...items)
        }
    }
    return entries
}
</script>

<template>
    <section class="flex flex-1 flex-col overflow-hidden text-sm">
        <Tabs v-model="activeTab" class="flex flex-1 flex-col overflow-hidden">
            <div class="shrink-0 px-4 pt-4">
                <TabsList
                    class="h-9 w-full justify-start gap-1 bg-transparent p-0"
                >
                    <TabsTrigger
                        v-for="server in appData.servers"
                        :key="server.info.id"
                        :value="server.info.id"
                        class="gap-1.5 data-[state=active]:bg-muted"
                    >
                        <span
                            class="size-2 rounded-full"
                            :style="{
                                backgroundColor: getServerColor(server.info.id),
                            }"
                        />
                        <span>{{ server.info.name }}</span>
                    </TabsTrigger>
                    <TabsTrigger
                        v-if="appData.backups.length"
                        value="backups"
                        class="gap-1.5 data-[state=active]:bg-muted"
                    >
                        <Archive class="size-3" />
                        <span>Backups</span>
                        <span class="text-xs text-muted-foreground"
                            >({{ appData.backups.length }})</span
                        >
                    </TabsTrigger>
                </TabsList>
            </div>

            <div class="flex-1 overflow-y-auto p-4">
                <TabsContent
                    v-for="server in appData.servers"
                    :key="server.info.id"
                    :value="server.info.id"
                    class="mt-0"
                >
                    <ServerSection
                        :server="server"
                        :source-kind="sourceKind"
                        :is-source="isSource"
                        :is-target="isTarget"
                        :all-backups="appData.backups"
                        @set-source="emit('setSource', $event)"
                        @add-target="emit('addTarget', $event)"
                        @backup="emit('backup', $event)"
                        @restore="
                            (entry, backup) => emit('restore', entry, backup)
                        "
                        @add-all-from-profile="
                            (p, k) => emit('addAllFromProfile', p, k)
                        "
                        @refresh="emit('refresh')"
                        @set-brackets-always-show="
                            (path, enabled) =>
                                emit('setBracketsAlwaysShow', path, enabled)
                        "
                    />
                </TabsContent>

                <TabsContent value="backups" class="mt-0">
                    <div class="mb-3 flex items-center gap-2">
                        <span class="text-lg font-semibold">Backups</span>
                        <span
                            class="rounded-full bg-muted px-2 py-0.5 text-xs text-muted-foreground"
                        >
                            {{ appData.backups.length }}
                        </span>
                    </div>
                    <div class="rounded-md border">
                        <Table>
                            <TableHeader>
                                <TableRow>
                                    <TableHead class="w-8"></TableHead>
                                    <TableHead
                                        class="cursor-pointer select-none"
                                        @click="toggleBackupSort('name')"
                                    >
                                        <div class="flex items-center gap-1">
                                            Name
                                            <ChevronUp
                                                v-if="
                                                    backupSortCol === 'name' &&
                                                    backupSortDir === 'asc'
                                                "
                                                class="size-3"
                                            />
                                            <ChevronDown
                                                v-else-if="
                                                    backupSortCol === 'name' &&
                                                    backupSortDir === 'desc'
                                                "
                                                class="size-3"
                                            />
                                        </div>
                                    </TableHead>
                                    <TableHead
                                        class="cursor-pointer select-none"
                                        @click="toggleBackupSort('time')"
                                    >
                                        <div class="flex items-center gap-1">
                                            Time
                                            <ChevronUp
                                                v-if="
                                                    backupSortCol === 'time' &&
                                                    backupSortDir === 'asc'
                                                "
                                                class="size-3"
                                            />
                                            <ChevronDown
                                                v-else-if="
                                                    backupSortCol === 'time' &&
                                                    backupSortDir === 'desc'
                                                "
                                                class="size-3"
                                            />
                                        </div>
                                    </TableHead>
                                    <TableHead class="w-20"></TableHead>
                                </TableRow>
                            </TableHeader>
                            <TableBody>
                                <BackupItem
                                    v-for="backup in sortedBackups"
                                    :key="backup.id"
                                    :backup="backup"
                                    :is-source="isBackupSource(backup)"
                                    :targets="getTargetsForBackup(backup)"
                                    @set-source="
                                        emit('setBackupSource', $event)
                                    "
                                    @delete="emit('deleteBackup', $event)"
                                    @apply="(b, t) => emit('applyBackup', b, t)"
                                />
                            </TableBody>
                        </Table>
                    </div>
                </TabsContent>
            </div>
        </Tabs>
    </section>
</template>
