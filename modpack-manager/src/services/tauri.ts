import { invoke } from '@tauri-apps/api/core';
import type { AppConfig, Launcher, MinecraftInstallation, UpdateReport, VerificationReport } from '../types/minecraft';

export const minecraftApi = {
  detect: () => invoke<MinecraftInstallation[]>('detect_minecraft_installations'),
  config: () => invoke<AppConfig>('get_config'),
  saveConfig: (config: AppConfig) => invoke<void>('save_config', { config }),
  validate: (path: string) => invoke<MinecraftInstallation>('validate_installation', { path }),
  verify: () => invoke<VerificationReport>('verify_manifest'),
  sync: () => invoke<UpdateReport>('sync_modpack'),
  launchers: () => invoke<Launcher[]>('detect_launchers'),
  openLauncher: () => invoke<Launcher>('launch_selected_launcher')
};
