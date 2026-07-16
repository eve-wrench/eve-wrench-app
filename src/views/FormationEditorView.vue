<script setup lang="ts">
import 'vue-sonner/style.css'
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { getAutoBackup } from '@/lib/settingsStore'
import { useColorMode } from '@vueuse/core'
import { Toaster } from '@/components/ui/sonner'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { ScrollArea } from '@/components/ui/scroll-area'
import {
    Plus,
    Copy,
    Trash2,
    X,
    Radar,
    ChevronUp,
    ChevronDown,
    Sun,
    Moon,
    Compass,
    Info,
    Scale,
} from 'lucide-vue-next'
import {
    Tooltip,
    TooltipContent,
    TooltipProvider,
    TooltipTrigger,
} from '@/components/ui/tooltip'
import { toast } from 'vue-sonner'
import {
    DropdownMenu,
    DropdownMenuContent,
    DropdownMenuItem,
    DropdownMenuSub,
    DropdownMenuSubContent,
    DropdownMenuSubTrigger,
    DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import WindowControls from '@/components/WindowControls.vue'
import { useWindowChrome } from '@/composables/useWindowChrome'
import type { FormationProbe, ProbeFormation } from '@/types'
import {
    FORMATION_PRESETS,
    STACK_PRESETS,
    type FormationPreset,
} from '@/lib/formationPresets'
import { useI18n } from '@/composables/useI18n'

const { t } = useI18n()
const colorMode = useColorMode()
const { isMac, isMaximized, minimize, toggleMaximize, close } =
    useWindowChrome()

const props = defineProps<{
    filePath: string
    entryName: string
}>()

const KM = 1000
const AU = 149597870700

// Edit state uses km for positions and AU for ranges; meters on the wire
type EditProbe = FormationProbe
type EditFormation = Omit<ProbeFormation, 'id'>
type Axis = 'x' | 'y' | 'z'

// EVE solarsystem coordinates (right-handed): +X West, +Y Up, +Z North.
// Each axis carries its compass poles (N/S, W/E, U/D), shown in both the
// coordinate table and the 3D scanner.
const AXES: { key: Axis; pos: string; neg: string; tip: string }[] = [
    { key: 'z', pos: 'N', neg: 'S', tip: 'axisNS' },
    { key: 'x', pos: 'W', neg: 'E', tip: 'axisWE' },
    { key: 'y', pos: 'U', neg: 'D', tip: 'axisUD' },
]
const AXIS_DIR: Record<Axis, [number, number, number]> = {
    x: [1, 0, 0],
    y: [0, 1, 0],
    z: [0, 0, 1],
}

const loading = ref(true)
const saving = ref(false)
const loadError = ref<string | null>(null)
const formations = ref<EditFormation[]>([])
const selected = ref(0)
const scaleFactor = ref(2)
const displayName = ref(props.entryName)
const savedSnapshot = ref('')
const diskChanged = ref(false)
let unlisten: UnlistenFn | null = null

const current = computed<EditFormation | null>(
    () => formations.value[selected.value] ?? null
)

const dirty = computed(
    () => JSON.stringify(formations.value) !== savedSnapshot.value
)

onMounted(async () => {
    await load()
    await refreshDisplayName()
    // The backend broadcasts data-changed to every window on any mutation
    unlisten = await listen('data-changed', onDataChanged)
})

onUnmounted(() => {
    unlisten?.()
})

async function readFormations(): Promise<EditFormation[]> {
    const data = await invoke<ProbeFormation[]>('read_probe_formations', {
        filePath: props.filePath,
    })
    return data.map((f) => ({
        name: f.name,
        probes: f.probes.map((p) => ({
            x: p.x / KM,
            y: p.y / KM,
            z: p.z / KM,
            range: p.range / AU,
        })),
    }))
}

async function load() {
    loading.value = true
    loadError.value = null
    formations.value = []
    selected.value = 0
    diskChanged.value = false
    try {
        formations.value = await readFormations()
        savedSnapshot.value = JSON.stringify(formations.value)
    } catch (e) {
        loadError.value = String(e)
    } finally {
        loading.value = false
    }
}

async function refreshDisplayName() {
    try {
        const name = await invoke<string>('get_entry_display_name', {
            filePath: props.filePath,
        })
        if (name && name !== displayName.value) {
            displayName.value = name
            await getCurrentWindow().setTitle(`Probe Formations — ${name}`)
        }
    } catch {
        // keep the name we have
    }
}

async function onDataChanged() {
    refreshDisplayName()
    let disk: EditFormation[]
    try {
        disk = await readFormations()
    } catch {
        return // file may be mid-write; a later event will catch up
    }
    const diskJson = JSON.stringify(disk)
    if (diskJson === savedSnapshot.value) {
        return // our file wasn't what changed
    }
    if (dirty.value) {
        diskChanged.value = true // don't clobber unsaved edits
        return
    }
    formations.value = disk
    savedSnapshot.value = diskJson
    selected.value = Math.max(
        0,
        Math.min(selected.value, formations.value.length - 1)
    )
}

async function save() {
    saving.value = true
    try {
        const payload: ProbeFormation[] = formations.value.map((f, i) => ({
            id: i,
            name: f.name.trim() || `Formation ${i + 1}`,
            probes: f.probes.map((p) => ({
                x: p.x * KM,
                y: p.y * KM,
                z: p.z * KM,
                range: p.range * AU,
            })),
        }))
        // Respect the "back up before changes" setting (shared via the store)
        await invoke('write_probe_formations', {
            filePath: props.filePath,
            formations: payload,
            backup: await getAutoBackup(),
        })
        // Normalize edit state to what a re-read would return
        formations.value.forEach((f, i) => {
            f.name = payload[i].name
        })
        savedSnapshot.value = JSON.stringify(formations.value)
        diskChanged.value = false
        toast.success(t('formationEditor.saved'), {
            description: t('formationEditor.savedDesc'),
        })
    } catch (e) {
        toast.error(t('formationEditor.saveFailed'), {
            description: String(e),
        })
    } finally {
        saving.value = false
    }
}

// Discard unsaved edits, reverting to the last saved state
function reset() {
    formations.value = JSON.parse(savedSnapshot.value)
    selected.value = Math.max(
        0,
        Math.min(selected.value, formations.value.length - 1)
    )
}

function addPreset(preset: FormationPreset) {
    const name =
        preset.id === 'blank'
            ? `Formation ${formations.value.length + 1}`
            : t(`formationEditor.presets.${preset.id}`)
    formations.value.push({ name, probes: preset.probes() })
    selected.value = formations.value.length - 1
}

function duplicateFormation() {
    if (!current.value) return
    formations.value.push({
        name: `${current.value.name} copy`,
        probes: current.value.probes.map((p) => ({ ...p })),
    })
    selected.value = formations.value.length - 1
}

function deleteFormation() {
    if (!current.value) return
    formations.value.splice(selected.value, 1)
    selected.value = Math.min(selected.value, formations.value.length - 1)
}

function moveFormation(i: number, dir: -1 | 1) {
    const j = i + dir
    if (j < 0 || j >= formations.value.length) return
    const arr = formations.value
    ;[arr[i], arr[j]] = [arr[j], arr[i]]
    if (selected.value === i) selected.value = j
    else if (selected.value === j) selected.value = i
}

// EVE probe launchers hold 8 probes, so formations are capped at 8
function addProbe() {
    if (!current.value || current.value.probes.length >= 8) return
    current.value.probes.push({ x: 0, y: 0, z: 0, range: 32 })
}

function removeProbe(index: number) {
    current.value?.probes.splice(index, 1)
}

function applyScale() {
    const n = scaleFactor.value
    if (!current.value || !isFinite(n) || n === 0) return
    // Negative means shrink by that factor: -2 divides by 2 (×0.5)
    const factor = n < 0 ? 1 / Math.abs(n) : n
    for (const p of current.value.probes) {
        p.x *= factor
        p.y *= factor
        p.z *= factor
    }
}

// Rotate all probes around the ship along one compass axis. Right-handed
// about the chosen axis, so positive degrees turn N→W around U/D. Rotation
// is linear, so a balanced formation stays balanced.
const rotateAxis = ref<Axis>('y')
const rotateDegrees = ref(45)

function applyRotation() {
    const f = current.value
    const deg = rotateDegrees.value
    if (!f || !isFinite(deg) || deg % 360 === 0) return
    const rad = (deg * Math.PI) / 180
    const cos = Math.cos(rad)
    const sin = Math.sin(rad)
    // Cyclic pairs keep the rotation right-handed: x:(y,z) y:(z,x) z:(x,y)
    const [a, b]: [Axis, Axis] =
        rotateAxis.value === 'x'
            ? ['y', 'z']
            : rotateAxis.value === 'y'
              ? ['z', 'x']
              : ['x', 'y']
    const round = (v: number) => Math.round(v * 1000) / 1000
    for (const p of f.probes) {
        const u = p[a]
        const v = p[b]
        p[a] = round(u * cos - v * sin)
        p[b] = round(u * sin + v * cos)
    }
}

function updateProbe(probe: EditProbe, key: keyof EditProbe, value: unknown) {
    const num = Number(value)
    if (isFinite(num)) probe[key] = num
}

// Valid probe scan ranges in EVE: powers of two from 0.25 to 32 AU
const RANGE_OPTIONS = [0.25, 0.5, 1, 2, 4, 8, 16, 32]

function rangeOptionsFor(value: number): number[] {
    // Keep unusual values from existing files selectable instead of lying
    return RANGE_OPTIONS.includes(value)
        ? RANGE_OPTIONS
        : [...RANGE_OPTIONS, value].sort((a, b) => a - b)
}

function setAllRanges(value: unknown) {
    const num = Number(value)
    if (!current.value || !isFinite(num)) return
    for (const p of current.value.probes) {
        p.range = num
    }
}

// ── Launch balancing ─────────────────────────────────────────────────────
//
// EVE re-centers a launched formation on its centroid (average probe
// position), so only zero-centroid formations launch exactly as drawn. A
// single counterweight probe on the opposite side zeroes the centroid.

const centroid = computed(() => {
    const probes = current.value?.probes ?? []
    if (!probes.length) return { x: 0, y: 0, z: 0 }
    const sum = { x: 0, y: 0, z: 0 }
    for (const p of probes) {
        sum.x += p.x
        sum.y += p.y
        sum.z += p.z
    }
    const n = probes.length
    return { x: sum.x / n, y: sum.y / n, z: sum.z / n }
})

// How far every probe drifts at launch when EVE pulls the centroid onto the ship
const launchShift = computed(() => {
    const c = centroid.value
    return Math.hypot(c.x, c.y, c.z)
})

// Sub-km drift is invisible at probe scales
const isBalanced = computed(() => launchShift.value < 0.5)

const canBalance = computed(
    () => (current.value?.probes.length ?? 0) > 0 && !isBalanced.value
)

// Zero the centroid with one counterweight probe: append one if the launcher
// has room, otherwise repurpose the last probe. The rest stay as drawn.
function balanceFormation() {
    const f = current.value
    if (!f || !canBalance.value) return
    const full = f.probes.length >= 8
    const rest = full ? f.probes.slice(0, -1) : f.probes
    const sum = { x: 0, y: 0, z: 0 }
    for (const p of rest) {
        sum.x += p.x
        sum.y += p.y
        sum.z += p.z
    }
    const round = (v: number) => Math.round(v * 1000) / 1000
    const counterweight = {
        x: round(-sum.x),
        y: round(-sum.y),
        z: round(-sum.z),
    }
    if (full) {
        Object.assign(f.probes[f.probes.length - 1], counterweight)
    } else {
        f.probes.push({ ...counterweight, range: rest[0].range })
    }
}

// ── Scanner projection ───────────────────────────────────────────────────

const yaw = ref(0.6)
const pitch = ref(0.4)
const zoom = ref(1)
let dragging = false
let lastX = 0
let lastY = 0

function setTextSelection(enabled: boolean) {
    const value = enabled ? '' : 'none'
    document.body.style.userSelect = value
    document.body.style.webkitUserSelect = value
}

function onPointerDown(e: PointerEvent) {
    dragging = true
    lastX = e.clientX
    lastY = e.clientY
    setTextSelection(false)
    ;(e.currentTarget as Element).setPointerCapture(e.pointerId)
}

function onPointerMove(e: PointerEvent) {
    if (!dragging) return
    yaw.value += (e.clientX - lastX) * 0.01
    pitch.value = Math.max(
        -Math.PI / 2,
        Math.min(Math.PI / 2, pitch.value + (e.clientY - lastY) * 0.01)
    )
    lastX = e.clientX
    lastY = e.clientY
}

function onPointerUp() {
    dragging = false
    setTextSelection(true)
}

function onWheel(e: WheelEvent) {
    zoom.value = Math.max(
        0.2,
        Math.min(4, zoom.value * (e.deltaY < 0 ? 1.1 : 0.9))
    )
}

function project(x: number, y: number, z: number) {
    const x1 = x * Math.cos(yaw.value) + z * Math.sin(yaw.value)
    const z1 = -x * Math.sin(yaw.value) + z * Math.cos(yaw.value)
    const y1 = y * Math.cos(pitch.value) - z1 * Math.sin(pitch.value)
    const depth = y * Math.sin(pitch.value) + z1 * Math.cos(pitch.value)
    return { sx: x1, sy: -y1, depth }
}

const extent = computed(() => {
    if (!current.value) return 1
    let max = 1
    for (const p of current.value.probes) {
        max = Math.max(max, Math.abs(p.x), Math.abs(p.y), Math.abs(p.z))
    }
    return max
})

const previewScale = computed(() => (160 / extent.value) * zoom.value)

// Compass-coloured axes, drawn subtle so the probes stay the focus
const sceneAxes = computed(() => {
    const e = extent.value
    const s = previewScale.value
    return AXES.map((a) => {
        const [dx, dy, dz] = AXIS_DIR[a.key]
        const from = project(-dx * e, -dy * e, -dz * e)
        const to = project(dx * e, dy * e, dz * e)
        const posEnd = project(dx * e * 1.16, dy * e * 1.16, dz * e * 1.16)
        const negEnd = project(-dx * e * 1.16, -dy * e * 1.16, -dz * e * 1.16)
        return {
            key: a.key,
            pos: a.pos,
            neg: a.neg,
            north: a.key === 'z',
            x1: from.sx * s,
            y1: from.sy * s,
            x2: to.sx * s,
            y2: to.sy * s,
            posX: posEnd.sx * s,
            posY: posEnd.sy * s,
            negX: negEnd.sx * s,
            negY: negEnd.sy * s,
        }
    })
})

// Round ring spacing to 1/2/2.5/5 × 10^n so the labels are nice numbers
function niceStep(target: number): number {
    const pow = Math.pow(10, Math.floor(Math.log10(Math.max(1, target))))
    for (const mult of [1, 2, 2.5, 5, 10]) {
        if (pow * mult >= target) return pow * mult
    }
    return pow * 10
}

function formatDistance(km: number): string {
    const AU_KM = 149597870.7
    if (km >= AU_KM / 100) return `${(km / AU_KM).toFixed(2)} AU`
    return `${Math.round(km).toLocaleString()} km`
}

const ringStep = computed(() => niceStep(extent.value / 3))

// Holographic scale rings in the horizontal (equatorial) plane
const rings = computed(() => {
    const step = ringStep.value
    const s = previewScale.value
    const SEGMENTS = 72
    return [step, step * 2, step * 3].map((radius) => {
        const points: string[] = []
        for (let i = 0; i < SEGMENTS; i++) {
            const angle = (i / SEGMENTS) * Math.PI * 2
            const p = project(
                radius * Math.cos(angle),
                0,
                radius * Math.sin(angle)
            )
            points.push(`${(p.sx * s).toFixed(1)},${(p.sy * s).toFixed(1)}`)
        }
        const labelPos = project(
            radius * Math.SQRT1_2,
            0,
            -radius * Math.SQRT1_2
        )
        return {
            radius,
            points: points.join(' '),
            labelX: labelPos.sx * s,
            labelY: labelPos.sy * s,
            label: formatDistance(radius),
        }
    })
})

// Faint radial spokes in the equatorial plane for a scanner-grid feel
const spokes = computed(() => {
    const r = ringStep.value * 3
    const s = previewScale.value
    return Array.from({ length: 8 }, (_, i) => {
        const angle = (i / 8) * Math.PI * 2
        const p = project(r * Math.cos(angle), 0, r * Math.sin(angle))
        return { x: p.sx * s, y: p.sy * s }
    })
})

// Each probe carries a tether down to its equatorial-plane shadow so height
// (north/south vs up/down) reads at a glance
const projectedProbes = computed(() => {
    if (!current.value) return []
    const s = previewScale.value
    return current.value.probes
        .map((p, index) => {
            const top = project(p.x, p.y, p.z)
            const base = project(p.x, 0, p.z)
            return {
                index,
                x: top.sx * s,
                y: top.sy * s,
                depth: top.depth,
                baseX: base.sx * s,
                baseY: base.sy * s,
                probe: p,
            }
        })
        .sort((a, b) => a.depth - b.depth)
})

// Where EVE will pull the formation's center at launch; hidden when balanced
const projectedCentroid = computed(() => {
    if (!current.value || isBalanced.value) return null
    const c = centroid.value
    const s = previewScale.value
    const p = project(c.x, c.y, c.z)
    return { x: p.sx * s, y: p.sy * s }
})

function toggleTheme() {
    colorMode.value = colorMode.value === 'dark' ? 'light' : 'dark'
}
</script>

<template>
    <div
        class="fixed inset-0 flex flex-col overflow-hidden bg-background text-foreground"
        :class="colorMode"
    >
        <Toaster
            position="top-center"
            rich-colors
            :theme="colorMode === 'dark' ? 'dark' : 'light'"
        />

        <!-- Custom titlebar, matching the main window's chrome -->
        <header
            data-tauri-drag-region
            class="flex h-11 shrink-0 items-center gap-2 border-b bg-background/80 px-3 backdrop-blur-sm"
        >
            <div class="pointer-events-none flex-1"></div>
            <div
                class="pointer-events-none flex min-w-0 items-center gap-2 px-2"
            >
                <Radar
                    class="size-4 shrink-0 text-foreground"
                    :stroke-width="2"
                />
                <h1 class="truncate text-xs font-semibold">
                    {{ t('formationEditor.title', { name: displayName }) }}
                </h1>
            </div>
            <div
                class="pointer-events-none flex flex-1 items-center justify-end gap-1"
            >
                <Button
                    variant="ghost"
                    size="icon"
                    :title="t('titleBar.toggleTheme')"
                    @click="toggleTheme"
                >
                    <Sun v-if="colorMode === 'dark'" class="size-4" />
                    <Moon v-else class="size-4" />
                </Button>
                <WindowControls
                    v-if="!isMac"
                    :is-maximized="isMaximized"
                    @minimize="minimize"
                    @toggle-maximize="toggleMaximize"
                    @close="close"
                />
            </div>
        </header>

        <div
            v-if="diskChanged"
            class="flex items-center justify-between gap-2 border-b bg-amber-500/10 px-4 py-1.5"
        >
            <span class="text-xs text-amber-600 dark:text-amber-400">
                {{ t('formationEditor.fileChanged') }}
            </span>
            <Button variant="outline" size="sm" class="h-6" @click="load">
                {{ t('formationEditor.reload') }}
            </Button>
        </div>

        <main
            v-if="loading"
            class="flex flex-1 items-center justify-center text-muted-foreground"
        >
            {{ t('formationEditor.loading') }}
        </main>

        <main
            v-else-if="loadError"
            class="flex flex-1 flex-col items-center justify-center gap-3 p-8 text-center"
        >
            <p class="font-medium">{{ t('formationEditor.loadFailed') }}</p>
            <p class="max-w-lg text-sm text-muted-foreground">
                {{ loadError }}
            </p>
            <Button variant="outline" size="sm" @click="load">
                {{ t('common.refresh') }}
            </Button>
        </main>

        <main v-else class="flex min-h-0 flex-1">
            <!-- Left: control console -->
            <aside
                class="flex w-[440px] shrink-0 flex-col border-r bg-muted/20"
            >
                <!-- Formations -->
                <section class="border-b px-4 pb-3 pt-4">
                    <div class="mb-2 flex items-center justify-between">
                        <span
                            class="text-[10px] font-medium uppercase tracking-[0.18em] text-muted-foreground"
                        >
                            {{ t('formationEditor.formations') }}
                        </span>
                        <div class="flex items-center gap-0.5">
                            <DropdownMenu>
                                <DropdownMenuTrigger as-child>
                                    <Button
                                        variant="ghost"
                                        size="icon"
                                        :title="
                                            t('formationEditor.addFormation')
                                        "
                                    >
                                        <Plus class="size-3.5" />
                                    </Button>
                                </DropdownMenuTrigger>
                                <DropdownMenuContent align="start">
                                    <DropdownMenuItem
                                        v-for="preset in FORMATION_PRESETS"
                                        :key="preset.id"
                                        @select="addPreset(preset)"
                                    >
                                        <component
                                            :is="preset.icon"
                                            class="mr-2 size-4"
                                        />
                                        {{
                                            t(
                                                `formationEditor.presets.${preset.id}`
                                            )
                                        }}
                                    </DropdownMenuItem>
                                    <DropdownMenuSub>
                                        <DropdownMenuSubTrigger>
                                            <Compass class="mr-2 size-4" />
                                            {{
                                                t(
                                                    'formationEditor.presetDirectional'
                                                )
                                            }}
                                        </DropdownMenuSubTrigger>
                                        <DropdownMenuSubContent>
                                            <DropdownMenuItem
                                                v-for="preset in STACK_PRESETS"
                                                :key="preset.id"
                                                @select="addPreset(preset)"
                                            >
                                                <component
                                                    :is="preset.icon"
                                                    class="mr-2 size-4"
                                                />
                                                {{
                                                    t(
                                                        `formationEditor.presets.${preset.id}`
                                                    )
                                                }}
                                            </DropdownMenuItem>
                                        </DropdownMenuSubContent>
                                    </DropdownMenuSub>
                                </DropdownMenuContent>
                            </DropdownMenu>
                            <Button
                                v-if="current"
                                variant="ghost"
                                size="icon"
                                :title="t('formationEditor.duplicate')"
                                @click="duplicateFormation"
                            >
                                <Copy class="size-3.5" />
                            </Button>
                            <Button
                                v-if="current"
                                variant="ghostDestructive"
                                size="icon"
                                :title="t('formationEditor.deleteFormation')"
                                @click="deleteFormation"
                            >
                                <Trash2 class="size-3.5" />
                            </Button>
                        </div>
                    </div>
                    <div
                        v-if="!formations.length"
                        class="py-4 text-center text-sm text-muted-foreground"
                    >
                        {{ t('formationEditor.noFormations') }}
                    </div>
                    <div v-else class="flex flex-col gap-px">
                        <div
                            v-for="(f, i) in formations"
                            :key="i"
                            class="group flex cursor-pointer items-center gap-2 rounded-md px-2 py-1.5 transition-colors"
                            :class="
                                i === selected
                                    ? 'bg-background shadow-sm'
                                    : 'hover:bg-background/50'
                            "
                            @click="selected = i"
                        >
                            <span
                                class="h-4 w-0.5 shrink-0 rounded-full transition-colors"
                                :class="
                                    i === selected
                                        ? 'bg-primary'
                                        : 'bg-transparent'
                                "
                            />
                            <input
                                v-model="f.name"
                                class="w-full min-w-0 bg-transparent text-sm outline-none"
                                :class="
                                    i === selected
                                        ? 'font-medium'
                                        : 'text-muted-foreground'
                                "
                                :placeholder="t('formationEditor.name')"
                                @focus="selected = i"
                            />
                            <div
                                class="flex shrink-0 items-center opacity-0 transition-opacity group-hover:opacity-100"
                            >
                                <Button
                                    variant="ghost"
                                    size="icon"
                                    :disabled="i === 0"
                                    :title="t('formationEditor.moveUp')"
                                    @click.stop="moveFormation(i, -1)"
                                >
                                    <ChevronUp class="size-3.5" />
                                </Button>
                                <Button
                                    variant="ghost"
                                    size="icon"
                                    :disabled="i === formations.length - 1"
                                    :title="t('formationEditor.moveDown')"
                                    @click.stop="moveFormation(i, 1)"
                                >
                                    <ChevronDown class="size-3.5" />
                                </Button>
                            </div>
                            <span
                                class="shrink-0 font-mono text-[10px] tabular-nums text-muted-foreground"
                            >
                                {{ f.probes.length }}/8
                            </span>
                        </div>
                    </div>
                </section>

                <!-- Probes -->
                <section
                    v-if="current"
                    class="flex min-h-0 flex-1 flex-col px-4 pb-2 pt-3"
                >
                    <div class="mb-2 flex items-center justify-between">
                        <span
                            class="text-[10px] font-medium uppercase tracking-[0.18em] text-muted-foreground"
                        >
                            {{ t('formationEditor.probes') }}
                        </span>
                        <Button
                            variant="ghost"
                            size="icon"
                            :disabled="current.probes.length >= 8"
                            :title="
                                current.probes.length >= 8
                                    ? t('formationEditor.maxProbes')
                                    : t('formationEditor.addProbe')
                            "
                            @click="addProbe"
                        >
                            <Plus class="size-3.5" />
                        </Button>
                    </div>
                    <div
                        class="grid grid-cols-[1.25rem_1fr_1fr_1fr_4.5rem_1.5rem] items-center gap-1 pb-1 text-[10px] uppercase tracking-wider text-muted-foreground"
                    >
                        <span></span>
                        <span
                            v-for="axis in AXES"
                            :key="axis.key"
                            class="text-right"
                            :title="t(`formationEditor.${axis.tip}`)"
                        >
                            {{ axis.pos }}/{{ axis.neg }}
                        </span>
                        <span class="text-center">AU</span>
                        <span></span>
                    </div>
                    <ScrollArea class="min-h-0 flex-1 pr-1">
                        <div class="flex flex-col gap-1">
                            <div
                                v-for="(p, i) in current.probes"
                                :key="i"
                                class="group grid grid-cols-[1.25rem_1fr_1fr_1fr_4.5rem_1.5rem] items-center gap-1"
                            >
                                <span
                                    class="font-mono text-[10px] tabular-nums text-muted-foreground"
                                >
                                    {{ String(i + 1).padStart(2, '0') }}
                                </span>
                                <Input
                                    v-for="axis in AXES"
                                    :key="axis.key"
                                    type="number"
                                    step="50"
                                    class="h-7 border-input/50 bg-background/60 px-1.5 text-right font-mono text-xs tabular-nums"
                                    :model-value="p[axis.key]"
                                    @update:model-value="
                                        updateProbe(p, axis.key, $event)
                                    "
                                />
                                <select
                                    :value="p.range"
                                    class="h-7 rounded-md border border-input/50 bg-background/60 px-1 text-center font-mono text-xs"
                                    @change="
                                        updateProbe(
                                            p,
                                            'range',
                                            ($event.target as HTMLSelectElement)
                                                .value
                                        )
                                    "
                                >
                                    <option
                                        v-for="r in rangeOptionsFor(p.range)"
                                        :key="r"
                                        :value="r"
                                    >
                                        {{ r }}
                                    </option>
                                </select>
                                <Button
                                    variant="ghostDestructive"
                                    size="icon"
                                    class="opacity-0 transition-opacity group-hover:opacity-100"
                                    @click="removeProbe(i)"
                                >
                                    <X class="size-3" />
                                </Button>
                            </div>
                        </div>
                    </ScrollArea>
                </section>

                <!-- Tools -->
                <section
                    v-if="current"
                    class="flex flex-col gap-2 border-t px-4 py-2.5"
                >
                    <div class="flex items-center gap-3">
                        <label class="flex items-center gap-1.5">
                            <span
                                class="text-[10px] uppercase tracking-wider text-muted-foreground"
                            >
                                {{ t('formationEditor.allRanges') }}
                            </span>
                            <select
                                value=""
                                class="h-7 rounded-md border border-input/50 bg-background/60 px-1.5 font-mono text-xs"
                                @change="
                                    setAllRanges(
                                        ($event.target as HTMLSelectElement)
                                            .value
                                    )
                                    ;(
                                        $event.target as HTMLSelectElement
                                    ).value = ''
                                "
                            >
                                <option value="" disabled>—</option>
                                <option
                                    v-for="r in RANGE_OPTIONS"
                                    :key="r"
                                    :value="r"
                                >
                                    {{ r }} AU
                                </option>
                            </select>
                        </label>
                        <label class="ml-auto flex items-center gap-1.5">
                            <span
                                class="text-[10px] uppercase tracking-wider text-muted-foreground"
                            >
                                {{ t('formationEditor.scale') }}
                            </span>
                            <Input
                                type="number"
                                step="0.5"
                                class="h-7 w-16 border-input/50 bg-background/60 px-1.5 text-right font-mono text-xs tabular-nums"
                                :model-value="scaleFactor"
                                @update:model-value="
                                    scaleFactor = Number($event)
                                "
                            />
                        </label>
                        <Button
                            variant="outline"
                            size="sm"
                            class="h-7"
                            @click="applyScale"
                        >
                            {{ t('formationEditor.apply') }}
                        </Button>
                    </div>
                    <div class="flex items-center gap-3">
                        <label class="flex items-center gap-1.5">
                            <span
                                class="text-[10px] uppercase tracking-wider text-muted-foreground"
                            >
                                {{ t('formationEditor.rotate') }}
                            </span>
                            <select
                                v-model="rotateAxis"
                                class="h-7 rounded-md border border-input/50 bg-background/60 px-1.5 font-mono text-xs"
                            >
                                <option
                                    v-for="axis in AXES"
                                    :key="axis.key"
                                    :value="axis.key"
                                    :title="t(`formationEditor.${axis.tip}`)"
                                >
                                    {{ axis.pos }}/{{ axis.neg }}
                                </option>
                            </select>
                        </label>
                        <label class="ml-auto flex items-center gap-1">
                            <Input
                                type="number"
                                step="15"
                                class="h-7 w-16 border-input/50 bg-background/60 px-1.5 text-right font-mono text-xs tabular-nums"
                                :model-value="rotateDegrees"
                                @update:model-value="
                                    rotateDegrees = Number($event)
                                "
                            />
                            <span class="text-xs text-muted-foreground">
                                °
                            </span>
                        </label>
                        <Button
                            variant="outline"
                            size="sm"
                            class="h-7"
                            @click="applyRotation"
                        >
                            {{ t('formationEditor.apply') }}
                        </Button>
                    </div>
                </section>

                <!-- Launch balance -->
                <section
                    v-if="current"
                    class="flex items-center gap-2 border-t px-4 py-2.5"
                >
                    <TooltipProvider :delay-duration="150">
                        <Tooltip>
                            <TooltipTrigger as-child>
                                <Info
                                    class="size-3.5 shrink-0 cursor-help text-muted-foreground"
                                />
                            </TooltipTrigger>
                            <TooltipContent side="top" class="max-w-72">
                                {{ t('formationEditor.balanceInfo') }}
                            </TooltipContent>
                        </Tooltip>
                    </TooltipProvider>
                    <span
                        v-if="isBalanced"
                        class="flex min-w-0 items-center gap-1.5 text-[11px] text-muted-foreground"
                    >
                        <span
                            class="size-1.5 shrink-0 rounded-full bg-emerald-500"
                        />
                        <span class="truncate">
                            {{ t('formationEditor.balanced') }}
                        </span>
                    </span>
                    <span
                        v-else
                        class="flex min-w-0 items-center gap-1.5 text-[11px] text-amber-600 dark:text-amber-400"
                    >
                        <span
                            class="size-1.5 shrink-0 rounded-full bg-amber-400"
                        />
                        <span class="truncate">
                            {{
                                t('formationEditor.offCenter', {
                                    offset: formatDistance(launchShift),
                                })
                            }}
                        </span>
                    </span>
                    <Button
                        variant="outline"
                        size="sm"
                        class="ml-auto h-7 shrink-0"
                        :disabled="!canBalance"
                        @click="balanceFormation"
                    >
                        <Scale class="size-3.5" />
                        {{ t('formationEditor.balance') }}
                    </Button>
                </section>

                <!-- Actions footer -->
                <div class="flex items-center gap-3 border-t px-4 py-3">
                    <Button
                        variant="outline"
                        size="sm"
                        :disabled="saving || !dirty"
                        @click="reset"
                    >
                        {{ t('formationEditor.reset') }}
                    </Button>
                    <span
                        v-if="dirty"
                        class="flex items-center gap-1.5 text-[11px] text-muted-foreground"
                    >
                        <span class="size-1.5 rounded-full bg-amber-400" />
                        {{ t('formationEditor.unsaved') }}
                    </span>
                    <Button
                        size="sm"
                        class="ml-auto min-w-28"
                        :disabled="saving || !dirty"
                        @click="save"
                    >
                        {{ t('formationEditor.save') }}
                    </Button>
                </div>
            </aside>

            <!-- Right: scanner viewport -->
            <div
                class="scanner relative min-w-0 flex-1 cursor-grab touch-none active:cursor-grabbing"
                @pointerdown="onPointerDown"
                @pointermove="onPointerMove"
                @pointerup="onPointerUp"
                @pointercancel="onPointerUp"
                @wheel.prevent="onWheel"
            >
                <svg
                    viewBox="-200 -200 400 400"
                    class="absolute inset-0 h-full w-full text-neutral-800 dark:text-neutral-100"
                >
                    <defs>
                        <filter
                            id="probe-shadow"
                            x="-120%"
                            y="-120%"
                            width="340%"
                            height="340%"
                        >
                            <feDropShadow
                                dx="0"
                                dy="0"
                                stdDeviation="2"
                                flood-color="#6b7280"
                                flood-opacity="0.85"
                            />
                        </filter>
                    </defs>

                    <!-- Equatorial spokes -->
                    <line
                        v-for="(sp, i) in spokes"
                        :key="`spoke-${i}`"
                        x1="0"
                        y1="0"
                        :x2="sp.x"
                        :y2="sp.y"
                        stroke="currentColor"
                        stroke-width="0.5"
                        opacity="0.12"
                    />

                    <!-- Range rings -->
                    <polygon
                        v-for="ring in rings"
                        :key="`ring-${ring.radius}`"
                        :points="ring.points"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="1"
                        stroke-dasharray="5 5"
                        opacity="0.28"
                    />
                    <text
                        v-for="ring in rings"
                        :key="`ring-label-${ring.radius}`"
                        :x="ring.labelX"
                        :y="ring.labelY"
                        fill="currentColor"
                        font-size="7"
                        font-family="ui-monospace, monospace"
                        opacity="0.75"
                        dx="3"
                        dy="-2"
                    >
                        {{ ring.label }}
                    </text>

                    <!-- Compass axes -->
                    <g v-for="axis in sceneAxes" :key="`axis-${axis.key}`">
                        <line
                            :x1="axis.x1"
                            :y1="axis.y1"
                            :x2="axis.x2"
                            :y2="axis.y2"
                            stroke="currentColor"
                            stroke-width="0.75"
                            opacity="0.4"
                        />
                        <text
                            :x="axis.posX"
                            :y="axis.posY"
                            fill="currentColor"
                            :font-size="axis.north ? 13 : 9"
                            :font-weight="axis.north ? 700 : 500"
                            :opacity="axis.north ? 1 : 0.85"
                            font-family="ui-monospace, monospace"
                            text-anchor="middle"
                            dominant-baseline="middle"
                        >
                            {{ axis.pos }}
                        </text>
                        <text
                            :x="axis.negX"
                            :y="axis.negY"
                            fill="currentColor"
                            font-size="9"
                            opacity="0.55"
                            font-family="ui-monospace, monospace"
                            text-anchor="middle"
                            dominant-baseline="middle"
                        >
                            {{ axis.neg }}
                        </text>
                    </g>

                    <!-- Center reticle -->
                    <g stroke="currentColor" opacity="0.5">
                        <line
                            x1="-4"
                            y1="0"
                            x2="4"
                            y2="0"
                            stroke-width="0.75"
                        />
                        <line
                            x1="0"
                            y1="-4"
                            x2="0"
                            y2="4"
                            stroke-width="0.75"
                        />
                    </g>

                    <!-- Launch center: the point EVE pulls onto the ship -->
                    <g
                        v-if="projectedCentroid"
                        class="text-amber-600 dark:text-amber-400"
                    >
                        <line
                            :x1="projectedCentroid.x"
                            :y1="projectedCentroid.y"
                            x2="0"
                            y2="0"
                            stroke="currentColor"
                            stroke-width="0.75"
                            stroke-dasharray="3 3"
                            opacity="0.6"
                        />
                        <g
                            :transform="`translate(${projectedCentroid.x} ${projectedCentroid.y})`"
                        >
                            <path
                                d="M0 -4 L4 0 L0 4 L-4 0 Z"
                                fill="none"
                                stroke="currentColor"
                                stroke-width="1"
                            />
                            <text
                                x="7"
                                y="1"
                                fill="currentColor"
                                font-size="7"
                                font-family="ui-monospace, monospace"
                                dominant-baseline="middle"
                            >
                                {{ t('formationEditor.launchCenter') }}
                            </text>
                        </g>
                    </g>

                    <!-- Probes: tether to plane shadow, then glowing node -->
                    <g v-for="p in projectedProbes" :key="p.index">
                        <line
                            :x1="p.x"
                            :y1="p.y"
                            :x2="p.baseX"
                            :y2="p.baseY"
                            stroke="currentColor"
                            stroke-width="0.75"
                            opacity="0.35"
                        />
                        <circle
                            :cx="p.baseX"
                            :cy="p.baseY"
                            r="1.5"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="0.75"
                            opacity="0.5"
                        />
                        <circle
                            :cx="p.x"
                            :cy="p.y"
                            :r="4.5 + p.depth / extent"
                            fill="#ffffff"
                            filter="url(#probe-shadow)"
                        >
                            <title>
                                #{{ p.index + 1 }}: ({{ p.probe.x }},
                                {{ p.probe.y }}, {{ p.probe.z }}) km —
                                {{ p.probe.range }} AU
                            </title>
                        </circle>
                    </g>
                </svg>

                <!-- HUD readout -->
                <div
                    class="pointer-events-none absolute left-3 top-3 flex flex-col gap-0.5 font-mono text-[10px] uppercase tracking-wider text-neutral-700/80 dark:text-white/70"
                >
                    <span
                        class="text-sm normal-case tracking-normal text-neutral-900 dark:text-white"
                    >
                        {{ current?.name }}
                    </span>
                    <span>{{ current?.probes.length ?? 0 }}/8 probes</span>
                    <span
                        >{{ t('formationEditor.zoom') }}
                        {{ Math.round(zoom * 100) }}%</span
                    >
                </div>

                <div
                    class="pointer-events-none absolute inset-x-0 bottom-3 flex justify-center"
                >
                    <span
                        class="rounded-full border border-neutral-400/40 bg-white/60 px-3 py-1 text-[11px] text-neutral-600 backdrop-blur dark:border-white/15 dark:bg-black/40 dark:text-white/60"
                    >
                        {{ t('formationEditor.previewHint') }}
                    </span>
                </div>
            </div>
        </main>
    </div>
</template>

<style scoped>
/* Instrument screen: a recessed scanner surface that follows the theme */
.scanner {
    background: radial-gradient(
        130% 130% at 50% 42%,
        #f0f0f0 0%,
        #e4e4e4 52%,
        #d4d4d4 100%
    );
}
.dark .scanner {
    background: radial-gradient(
        130% 130% at 50% 42%,
        #191919 0%,
        #0d0d0d 52%,
        #060606 100%
    );
}

/* Arrow keys still step; the spinner chrome fights the compact grid */
input[type='number']::-webkit-outer-spin-button,
input[type='number']::-webkit-inner-spin-button {
    -webkit-appearance: none;
    margin: 0;
}
input[type='number'] {
    -moz-appearance: textfield;
    appearance: textfield;
}
</style>
