export type MinecraftInstallation = { name: string; path: string; valid: boolean; reason?: string };
export type AppConfig = { installationPath?: string; minecraftPath?: string; manifestUrl?: string; launcherPath?: string; launcherName?: string; reduceMotion: boolean; particles: boolean };
export type VerificationReport = { modpackVersion: string; checked: number; valid: number; missing: string[]; changed: string[]; invalid: string[]; obsolete: string[] };
export type UpdateReport = { modpackVersion: string; downloaded: string[]; updated: string[]; alreadyValid: number; archived: string[] };
export type Launcher = { name: string; path: string };
