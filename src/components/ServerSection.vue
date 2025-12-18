<script setup lang="ts">
import { Switch } from '@/components/ui/switch'
import ProfileCard from './ProfileCard.vue'
import type {
    ServerData,
    ProfileData,
    SettingsEntry,
    SettingsKind,
    BackupEntry,
} from '@/types'

const props = defineProps<{
    server: ServerData
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
    setBracketsAlwaysShow: [serverPath: string, enabled: boolean]
}>()
</script>

<template>
    <div class="space-y-6">
        <ProfileCard
            v-for="profile in server.profiles"
            :key="profile.path"
            :profile="profile"
            :source-kind="sourceKind"
            :is-source="isSource"
            :is-target="isTarget"
            :all-backups="allBackups"
            @set-source="emit('setSource', $event)"
            @add-target="emit('addTarget', $event)"
            @backup="emit('backup', $event)"
            @restore="(entry, backup) => emit('restore', entry, backup)"
            @add-all-from-profile="(p, k) => emit('addAllFromProfile', p, k)"
            @refresh="emit('refresh')"
        />

        <!-- Extra Settings -->
        <div class="mt-8">
            <div class="mb-3">
                <span class="text-lg font-semibold">Extra</span>
            </div>
            <div class="rounded-md border p-4">
                <div class="flex items-center justify-between gap-4">
                    <div class="flex flex-col gap-1">
                        <span class="text-sm font-medium"
                            >Always Show Bracket Text</span
                        >
                        <span class="text-xs text-muted-foreground">
                            Show ship labels on all brackets in space, not just
                            selected targets. May impact performance with 200+
                            pilots on grid. Requires client restart.
                        </span>
                    </div>
                    <Switch
                        :model-value="props.server.info.brackets_always_show"
                        @update:model-value="
                            emit(
                                'setBracketsAlwaysShow',
                                props.server.info.server_path,
                                $event
                            )
                        "
                    />
                </div>
            </div>
        </div>
    </div>
</template>
