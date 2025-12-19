import { ref, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'

export interface UpdateInfo {
    current_version: string
    latest_version: string
    release_url: string
    release_notes: string
}

export function useUpdateChecker() {
    const updateAvailable = ref(false)
    const updateInfo = ref<UpdateInfo | null>(null)
    const dismissed = ref(false)

    async function checkForUpdate() {
        try {
            const info = await invoke<UpdateInfo | null>('check_for_update')
            if (info) {
                updateInfo.value = info
                updateAvailable.value = true
            }
        } catch {
            // Silently fail
        }
    }

    function dismiss() {
        dismissed.value = true
    }

    onMounted(() => {
        checkForUpdate()
    })

    return {
        updateAvailable,
        updateInfo,
        dismissed,
        dismiss,
        checkForUpdate,
    }
}
