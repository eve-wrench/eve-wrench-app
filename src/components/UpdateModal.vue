<script setup lang="ts">
import { Download, X, ExternalLink } from 'lucide-vue-next'
import { openUrl } from '@tauri-apps/plugin-opener'
import { Button } from '@/components/ui/button'
import type { UpdateInfo } from '@/composables/useUpdateChecker'

const props = defineProps<{
    info: UpdateInfo
}>()

const emit = defineEmits<{
    dismiss: []
}>()

function openRelease() {
    openUrl(props.info.release_url)
}
</script>

<template>
    <div
        class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
        @click.self="emit('dismiss')"
    >
        <div
            class="mx-4 w-full max-w-md overflow-hidden rounded-lg border bg-background shadow-2xl"
        >
            <div class="flex items-center justify-between border-b p-4">
                <div class="flex items-center gap-3">
                    <div
                        class="flex size-10 items-center justify-center rounded-lg bg-primary/20"
                    >
                        <Download class="size-5 text-primary" />
                    </div>
                    <div>
                        <h2 class="font-semibold">Update Available</h2>
                        <p class="text-xs text-muted-foreground">
                            v{{ info.current_version }} → v{{
                                info.latest_version
                            }}
                        </p>
                    </div>
                </div>
                <Button
                    variant="ghost"
                    size="icon"
                    class="size-8"
                    @click="emit('dismiss')"
                >
                    <X class="size-5" />
                </Button>
            </div>

            <div class="p-4">
                <p class="mb-4 text-sm text-muted-foreground">
                    A new version of EVE Wrench is available. Update to get the
                    latest features and bug fixes.
                </p>

                <div
                    v-if="info.release_notes"
                    class="mb-4 max-h-32 overflow-auto rounded-lg bg-muted p-3 text-xs text-muted-foreground"
                >
                    <p class="whitespace-pre-wrap">
                        {{
                            info.release_notes.slice(0, 300) +
                            (info.release_notes.length > 300 ? '...' : '')
                        }}
                    </p>
                </div>
            </div>

            <div class="flex gap-3 border-t bg-muted/30 p-4">
                <Button
                    variant="outline"
                    class="flex-1"
                    @click="emit('dismiss')"
                >
                    Later
                </Button>
                <Button class="flex-1 gap-2" @click="openRelease">
                    <Download class="size-4" />
                    Download
                    <ExternalLink class="size-3 opacity-70" />
                </Button>
            </div>
        </div>
    </div>
</template>
