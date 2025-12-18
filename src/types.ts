export type SettingsKind = 'user' | 'char'
export type ServerId =
    | 'tranquility'
    | 'singularity'
    | 'thunderdome'
    | 'serenity'

export interface CharacterDetails {
    name: string
    corporation: string | null
    portrait_url: string
}

export interface SettingsEntry {
    path: string
    id: string
    kind: SettingsKind
    server: ServerId
    profile: string
    display_name: string
    character: CharacterDetails | null
    alias: string | null
    modified_time: number
    relative_time: string
}

export interface ProfileData {
    name: string
    path: string
    accounts: SettingsEntry[]
    characters: SettingsEntry[]
}

export interface ServerInfo {
    id: ServerId
    name: string
    short_name: string
    color: string
    supports_esi: boolean
    brackets_always_show: boolean
    server_path: string
}

export interface ServerData {
    info: ServerInfo
    profiles: ProfileData[]
}

export interface BackupEntry {
    id: string
    name: string
    path: string
    timestamp: number
    kind: SettingsKind
    original_id: string
    original_name: string | null
    display_name: string
    relative_time: string
}

export interface AppData {
    servers: ServerData[]
    backups: BackupEntry[]
}

export type SourceItem = SettingsEntry | BackupEntry

export function isBackup(item: SourceItem): item is BackupEntry {
    return 'original_id' in item
}

export function getServerColor(serverId: ServerId): string {
    const colors: Record<ServerId, string> = {
        tranquility: 'hsl(160, 100%, 40%)',
        singularity: 'hsl(280, 80%, 60%)',
        thunderdome: 'hsl(35, 100%, 50%)',
        serenity: 'hsl(200, 80%, 50%)',
    }
    return colors[serverId] || 'hsl(0, 0%, 50%)'
}

export function getServerShortName(serverId: ServerId): string {
    const names: Record<ServerId, string> = {
        tranquility: 'TQ',
        singularity: 'SISI',
        thunderdome: 'TD',
        serenity: 'CN',
    }
    return names[serverId] || serverId
}
