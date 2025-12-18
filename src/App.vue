<script setup lang="ts">
import 'vue-sonner/style.css'
import { onMounted } from 'vue'
import { useColorMode } from '@vueuse/core'
import { Toaster } from '@/components/ui/sonner'
import { TooltipProvider } from '@/components/ui/tooltip'
import { Rocket } from 'lucide-vue-next'
import TitleBar from '@/components/TitleBar.vue'
import SettingsBrowser from '@/components/SettingsBrowser.vue'
import CopyPanel from '@/components/CopyPanel.vue'
import ConfirmDialog from '@/components/ConfirmDialog.vue'
import PromptDialog from '@/components/PromptDialog.vue'
import { useCopyManager } from '@/composables/useCopyManager'
import { isBackup } from '@/types'

const colorMode = useColorMode()

const {
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
    restoreBackup,
    applyBackup,
    isSource,
    isTarget,
    setBracketsAlwaysShow,
} = useCopyManager()

function isBackupSource(backup: { id: string }): boolean {
    return !!(
        source.value &&
        isBackup(source.value) &&
        source.value.id === backup.id
    )
}

function toggleDarkMode() {
    colorMode.value = colorMode.value === 'dark' ? 'light' : 'dark'
}

onMounted(init)
</script>

<template>
    <TooltipProvider>
        <div
            class="fixed inset-0 flex flex-col overflow-hidden bg-background"
            :class="colorMode"
        >
            <Toaster
                position="top-center"
                rich-colors
                :theme="colorMode === 'dark' ? 'dark' : 'light'"
            />
            <ConfirmDialog />
            <PromptDialog />

            <TitleBar
                :loading="loading"
                :color-mode="colorMode"
                @refresh="refresh"
                @toggle-theme="toggleDarkMode"
            />

            <main class="flex flex-1 overflow-hidden">
                <div
                    v-if="loading && !appData"
                    class="flex flex-1 items-center justify-center"
                >
                    <div class="flex flex-col items-center gap-3">
                        <div
                            class="size-8 animate-spin rounded-full border-2 border-muted border-t-primary"
                        />
                        <p class="text-sm text-muted-foreground">
                            Scanning EVE installations...
                        </p>
                    </div>
                </div>

                <template v-else-if="!hasData">
                    <div class="flex flex-1 items-center justify-center">
                        <div
                            class="flex flex-col items-center gap-3 text-center"
                        >
                            <Rocket class="size-12 text-muted-foreground" />
                            <h3 class="font-semibold">
                                No EVE Installations Found
                            </h3>
                            <p class="text-sm text-muted-foreground">
                                Make sure EVE Online is installed and has been
                                launched at least once.
                            </p>
                        </div>
                    </div>
                </template>

                <template v-else-if="appData">
                    <SettingsBrowser
                        :app-data="appData"
                        :source-kind="sourceKind"
                        :is-source="isSource"
                        :is-target="isTarget"
                        :is-backup-source="isBackupSource"
                        @set-source="setSource"
                        @add-target="addTarget"
                        @backup="createBackup"
                        @restore="restoreBackup"
                        @apply-backup="applyBackup"
                        @add-all-from-profile="addAllFromProfile"
                        @set-backup-source="setSource"
                        @delete-backup="deleteBackup"
                        @refresh="refresh"
                        @set-brackets-always-show="setBracketsAlwaysShow"
                    />

                    <CopyPanel
                        :source="source"
                        :targets="targets"
                        :can-copy="canCopy"
                        :copying="copying"
                        @clear-source="clearSource"
                        @remove-target="removeTarget"
                        @clear-targets="clearTargets"
                        @execute-copy="executeCopy"
                    />
                </template>
            </main>
        </div>
    </TooltipProvider>
</template>
