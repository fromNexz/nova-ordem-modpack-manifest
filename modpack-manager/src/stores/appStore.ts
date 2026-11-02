import { create } from 'zustand';
import type { AppConfig, MinecraftInstallation } from '../types/minecraft';

type State = { config: AppConfig | null; installations: MinecraftInstallation[]; loading: boolean; error?: string; selected?: MinecraftInstallation; setSelected: (item?: MinecraftInstallation) => void; initialize: () => Promise<void> };
export const useAppStore = create<State>((set) => ({
  config: null, installations: [], loading: true,
  setSelected: (selected) => set({ selected }),
  initialize: async () => {
    try {
      const api = await import('../services/tauri');
      const [config, installations] = await Promise.all([api.minecraftApi.config(), api.minecraftApi.detect()]);
      set({ config, installations, selected: installations.find(i => i.path === config.installationPath), loading: false });
    } catch (error) { set({ error: error instanceof Error ? error.message : 'Não foi possível acessar o Minecraft.', loading: false }); }
  }
}));
