<script setup lang="ts">
import { ref, computed } from 'vue'
import { Button } from '@/components/ui/button'
import {
    Table,
    TableBody,
    TableHead,
    TableHeader,
    TableRow,
} from '@/components/ui/table'
import { ChevronUp, ChevronDown } from 'lucide-vue-next'
import AccountItem from './AccountItem.vue'
import SettingsItem from './SettingsItem.vue'
import type {
    ProfileData,
    SettingsEntry,
    SettingsKind,
    BackupEntry,
} from '@/types'

const props = defineProps<{
    profile: ProfileData
    sourceKind: SettingsKind | null
    isSource: (entry: SettingsEntry) => boolean
    isTarget: (entry: SettingsEntry) => boolean
    allBackups: BackupEntry[]
}>()

const emit = defineEmits<{
    setSource: [entry: SettingsEntry]
    addTarget: [entry: SettingsEntry]
    backup: [entry: SettingsEntry]
    restore: [entry: SettingsEntry, backup: BackupEntry]
    addAllFromProfile: [profile: ProfileData, kind: SettingsKind]
    refresh: []
}>()

const showProfileName = computed(
    () => props.profile.name.toLowerCase() !== 'default'
)

type SortColumn = 'name' | 'modified'
type SortDirection = 'asc' | 'desc'

const accountSortCol = ref<SortColumn>('name')
const accountSortDir = ref<SortDirection>('asc')
const charSortCol = ref<SortColumn>('name')
const charSortDir = ref<SortDirection>('asc')

function toggleAccountSort(col: SortColumn) {
    if (accountSortCol.value === col) {
        accountSortDir.value = accountSortDir.value === 'asc' ? 'desc' : 'asc'
    } else {
        accountSortCol.value = col
        accountSortDir.value = 'asc'
    }
}

function toggleCharSort(col: SortColumn) {
    if (charSortCol.value === col) {
        charSortDir.value = charSortDir.value === 'asc' ? 'desc' : 'asc'
    } else {
        charSortCol.value = col
        charSortDir.value = 'asc'
    }
}

function sortEntries(
    entries: SettingsEntry[],
    col: SortColumn,
    dir: SortDirection,
    isAccount: boolean
): SettingsEntry[] {
    return [...entries].sort((a, b) => {
        let cmp = 0
        if (col === 'name') {
            const nameA = isAccount
                ? a.alias || a.id
                : a.character?.name || a.id
            const nameB = isAccount
                ? b.alias || b.id
                : b.character?.name || b.id
            cmp = nameA.localeCompare(nameB)
        } else {
            cmp = a.modified_time - b.modified_time
        }
        return dir === 'asc' ? cmp : -cmp
    })
}

function getBackupsForEntry(entry: SettingsEntry): BackupEntry[] {
    return props.allBackups.filter(
        (b) => b.kind === entry.kind && b.original_id === entry.id
    )
}

const sortedAccounts = computed(() =>
    sortEntries(
        props.profile.accounts,
        accountSortCol.value,
        accountSortDir.value,
        true
    )
)

const sortedCharacters = computed(() =>
    sortEntries(
        props.profile.characters,
        charSortCol.value,
        charSortDir.value,
        false
    )
)
</script>

<template>
    <div class="space-y-6 text-sm">
        <div v-if="profile.accounts.length">
            <div class="mb-3 flex items-center gap-2">
                <span class="text-lg font-semibold">Accounts</span>
                <span v-if="showProfileName" class="text-muted-foreground"
                    >· {{ profile.name }}</span
                >
                <span
                    class="rounded-full bg-muted px-2 py-0.5 text-xs text-muted-foreground"
                >
                    {{ profile.accounts.length }}
                </span>
            </div>
            <div class="rounded-md border">
                <Table>
                    <TableHeader>
                        <TableRow>
                            <TableHead class="w-8"></TableHead>
                            <TableHead
                                class="cursor-pointer select-none"
                                @click="toggleAccountSort('name')"
                            >
                                <div class="flex items-center gap-1">
                                    Name
                                    <ChevronUp
                                        v-if="
                                            accountSortCol === 'name' &&
                                            accountSortDir === 'asc'
                                        "
                                        class="size-3"
                                    />
                                    <ChevronDown
                                        v-else-if="
                                            accountSortCol === 'name' &&
                                            accountSortDir === 'desc'
                                        "
                                        class="size-3"
                                    />
                                </div>
                            </TableHead>
                            <TableHead
                                class="cursor-pointer select-none"
                                @click="toggleAccountSort('modified')"
                            >
                                <div class="flex items-center gap-1">
                                    Modified
                                    <ChevronUp
                                        v-if="
                                            accountSortCol === 'modified' &&
                                            accountSortDir === 'asc'
                                        "
                                        class="size-3"
                                    />
                                    <ChevronDown
                                        v-else-if="
                                            accountSortCol === 'modified' &&
                                            accountSortDir === 'desc'
                                        "
                                        class="size-3"
                                    />
                                </div>
                            </TableHead>
                            <TableHead class="w-24 text-right">
                                <Button
                                    v-if="sourceKind === 'user'"
                                    variant="ghost"
                                    size="sm"
                                    class="h-6 text-xs"
                                    @click="
                                        emit(
                                            'addAllFromProfile',
                                            profile,
                                            'user'
                                        )
                                    "
                                >
                                    Add all
                                </Button>
                            </TableHead>
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        <AccountItem
                            v-for="account in sortedAccounts"
                            :key="account.path"
                            :entry="account"
                            :is-source="isSource(account)"
                            :is-target="isTarget(account)"
                            :source-kind="sourceKind"
                            :backups="getBackupsForEntry(account)"
                            @set-source="emit('setSource', $event)"
                            @add-target="emit('addTarget', $event)"
                            @backup="emit('backup', $event)"
                            @restore="
                                (entry, backup) =>
                                    emit('restore', entry, backup)
                            "
                            @alias-changed="emit('refresh')"
                        />
                    </TableBody>
                </Table>
            </div>
        </div>

        <div v-if="profile.characters.length">
            <div class="mb-3 mt-8 flex items-center gap-2">
                <span class="text-lg font-semibold">Characters</span>
                <span v-if="showProfileName" class="text-muted-foreground"
                    >· {{ profile.name }}</span
                >
                <span
                    class="rounded-full bg-muted px-2 py-0.5 text-xs text-muted-foreground"
                >
                    {{ profile.characters.length }}
                </span>
            </div>
            <div class="rounded-md border">
                <Table>
                    <TableHeader>
                        <TableRow>
                            <TableHead class="w-8"></TableHead>
                            <TableHead
                                class="cursor-pointer select-none"
                                @click="toggleCharSort('name')"
                            >
                                <div class="flex items-center gap-1">
                                    Name
                                    <ChevronUp
                                        v-if="
                                            charSortCol === 'name' &&
                                            charSortDir === 'asc'
                                        "
                                        class="size-3"
                                    />
                                    <ChevronDown
                                        v-else-if="
                                            charSortCol === 'name' &&
                                            charSortDir === 'desc'
                                        "
                                        class="size-3"
                                    />
                                </div>
                            </TableHead>
                            <TableHead
                                class="cursor-pointer select-none"
                                @click="toggleCharSort('modified')"
                            >
                                <div class="flex items-center gap-1">
                                    Modified
                                    <ChevronUp
                                        v-if="
                                            charSortCol === 'modified' &&
                                            charSortDir === 'asc'
                                        "
                                        class="size-3"
                                    />
                                    <ChevronDown
                                        v-else-if="
                                            charSortCol === 'modified' &&
                                            charSortDir === 'desc'
                                        "
                                        class="size-3"
                                    />
                                </div>
                            </TableHead>
                            <TableHead class="w-24 text-right">
                                <Button
                                    v-if="sourceKind === 'char'"
                                    variant="ghost"
                                    size="sm"
                                    class="h-6 text-xs"
                                    @click="
                                        emit(
                                            'addAllFromProfile',
                                            profile,
                                            'char'
                                        )
                                    "
                                >
                                    Add all
                                </Button>
                            </TableHead>
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        <SettingsItem
                            v-for="char in sortedCharacters"
                            :key="char.path"
                            :entry="char"
                            :is-source="isSource(char)"
                            :is-target="isTarget(char)"
                            :source-kind="sourceKind"
                            :backups="getBackupsForEntry(char)"
                            @set-source="emit('setSource', $event)"
                            @add-target="emit('addTarget', $event)"
                            @backup="emit('backup', $event)"
                            @restore="
                                (entry, backup) =>
                                    emit('restore', entry, backup)
                            "
                            @alias-changed="emit('refresh')"
                        />
                    </TableBody>
                </Table>
            </div>
        </div>
    </div>
</template>
