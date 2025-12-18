<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { platform } from '@tauri-apps/plugin-os'
import {
    Wrench,
    Minus,
    Square,
    X,
    Copy,
    Sun,
    Moon,
    RefreshCw,
} from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import {
    Tooltip,
    TooltipContent,
    TooltipTrigger,
} from '@/components/ui/tooltip'

defineProps<{
    loading: boolean
    colorMode: string
}>()

const emit = defineEmits<{
    refresh: []
    toggleTheme: []
}>()

const appWindow = getCurrentWindow()
const isMac = ref(true)
const isMaximized = ref(false)

onMounted(async () => {
    isMac.value = platform() === 'macos'
    isMaximized.value = await appWindow.isMaximized()

    if (!isMac.value) {
        await appWindow.setDecorations(false)
    }

    appWindow.onResized(async () => {
        isMaximized.value = await appWindow.isMaximized()
    })
})

async function minimize() {
    await appWindow.minimize()
}

async function toggleMaximize() {
    await appWindow.toggleMaximize()
}

async function close() {
    await appWindow.close()
}
</script>

<template>
    <header
        data-tauri-drag-region
        class="titlebar flex h-11 shrink-0 items-center justify-between border-b bg-background/80 px-4 backdrop-blur-sm"
    >
        <!-- Left: Spacer for macOS traffic lights -->
        <div class="w-20 shrink-0"></div>

        <!-- Center: Logo -->
        <div data-tauri-drag-region class="flex items-center gap-2">
            <Wrench class="size-4 text-primary" :stroke-width="2" />
            <span
                class="text-xs font-semibold tracking-widest text-muted-foreground"
            >
                EVE WRENCH
            </span>
        </div>

        <!-- Right: Actions + Window controls -->
        <div class="flex shrink-0 items-center justify-end gap-1">
            <Tooltip>
                <TooltipTrigger as-child>
                    <Button
                        variant="ghost"
                        size="icon"
                        class="size-8"
                        @click="emit('toggleTheme')"
                    >
                        <Sun v-if="colorMode === 'dark'" class="size-4" />
                        <Moon v-else class="size-4" />
                    </Button>
                </TooltipTrigger>
                <TooltipContent>Toggle theme</TooltipContent>
            </Tooltip>

            <Tooltip>
                <TooltipTrigger as-child>
                    <Button
                        variant="ghost"
                        size="icon"
                        class="size-8"
                        :disabled="loading"
                        @click="emit('refresh')"
                    >
                        <RefreshCw
                            class="size-4"
                            :class="{ 'animate-spin': loading }"
                        />
                    </Button>
                </TooltipTrigger>
                <TooltipContent>Refresh</TooltipContent>
            </Tooltip>

            <!-- Window controls (Windows/Linux only) -->
            <template v-if="!isMac">
                <div class="ml-2 flex">
                    <button
                        class="flex size-8 items-center justify-center text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
                        title="Minimize"
                        @click="minimize"
                    >
                        <Minus class="size-4" :stroke-width="1.5" />
                    </button>
                    <button
                        class="flex size-8 items-center justify-center text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
                        :title="isMaximized ? 'Restore' : 'Maximize'"
                        @click="toggleMaximize"
                    >
                        <Copy
                            v-if="isMaximized"
                            class="size-3.5"
                            :stroke-width="1.5"
                        />
                        <Square v-else class="size-3" :stroke-width="1.5" />
                    </button>
                    <button
                        class="flex size-8 items-center justify-center text-muted-foreground transition-colors hover:bg-destructive hover:text-white"
                        title="Close"
                        @click="close"
                    >
                        <X class="size-4" :stroke-width="1.5" />
                    </button>
                </div>
            </template>
        </div>
    </header>
</template>

<style>
[data-tauri-drag-region] {
    -webkit-user-select: none;
    user-select: none;
    -webkit-app-region: drag;
    app-region: drag;
}

[data-tauri-drag-region] * {
    -webkit-user-select: none;
    user-select: none;
}

[data-tauri-drag-region] span,
[data-tauri-drag-region] svg {
    pointer-events: none;
}

[data-tauri-drag-region] button,
[data-tauri-drag-region] [role='button'],
[data-tauri-drag-region] a,
[data-tauri-drag-region] input,
[data-tauri-drag-region] [data-no-drag] {
    -webkit-app-region: no-drag;
    app-region: no-drag;
    pointer-events: auto;
}
</style>
