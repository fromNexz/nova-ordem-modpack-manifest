import { useEffect, useState } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import { Compass, Download, FolderSearch, Settings, ShieldCheck, Sparkles } from 'lucide-react';
import { useAppStore } from './stores/appStore';
import { minecraftApi } from './services/tauri';
import type { Launcher, MinecraftInstallation, UpdateReport, VerificationReport } from './types/minecraft';

const OFFICIAL_MANIFEST_URL = 'https://raw.githubusercontent.com/fromNexz/nova-ordem-modpack-manifest/main/manifest.json';

export default function App() {
  const { config, installations, selected, loading, error, setSelected, initialize } = useAppStore();
  const [setupOpen, setSetupOpen] = useState(false);
  const [saving, setSaving] = useState(false);
  const [report, setReport] = useState<VerificationReport>();
  const [update, setUpdate] = useState<UpdateReport>();
  const [checking, setChecking] = useState(false);
  const [updating, setUpdating] = useState(false);
  const [verifyError, setVerifyError] = useState<string>();
  const [lastChecked, setLastChecked] = useState<Date>();
  useEffect(() => { void initialize(); }, [initialize]);

  async function choose(installation: MinecraftInstallation) {
    setSelected(installation); setSaving(true);
    try { await minecraftApi.saveConfig({ installationPath: installation.path, minecraftPath: config?.minecraftPath, manifestUrl: config?.manifestUrl, launcherPath: config?.launcherPath, launcherName: config?.launcherName, particles: config?.particles ?? true, reduceMotion: config?.reduceMotion ?? false }); setSetupOpen(false); await initialize(); }
    finally { setSaving(false); }
  }
  async function verify() {
    const manifestUrl = config?.manifestUrl ?? OFFICIAL_MANIFEST_URL;
    if (!config?.manifestUrl) { await minecraftApi.saveConfig({ installationPath: config?.installationPath, minecraftPath: config?.minecraftPath, manifestUrl, particles: config?.particles ?? true, reduceMotion: config?.reduceMotion ?? false }); await initialize(); }
    setVerifyError(undefined); setUpdate(undefined); setChecking(true);
    try {
      const initial = await minecraftApi.verify(); setReport(initial); setLastChecked(new Date());
      const needsRepair = initial.missing.length + initial.changed.length + initial.invalid.length + initial.obsolete.length;
      if (needsRepair > 0) {
        setUpdating(true); const result = await minecraftApi.sync(); setUpdate(result);
        const repaired = await minecraftApi.verify(); setReport(repaired); setLastChecked(new Date());
        if (repaired.missing.length + repaired.changed.length + repaired.invalid.length + repaired.obsolete.length === 0 && config?.launcherPath) await minecraftApi.openLauncher();
      }
    } catch (e) { setVerifyError(e instanceof Error ? e.message : String(e)); }
    finally { setUpdating(false); setChecking(false); }
  }
  async function openLauncher() {
    setVerifyError(undefined);
    try { await minecraftApi.openLauncher(); }
    catch (e) { setVerifyError(e instanceof Error ? e.message : 'Não foi possível abrir o launcher.'); }
  }
  async function chooseLauncher() {
    const found: Launcher[] = await minecraftApi.launchers();
    const manual = await open({ multiple: false, directory: false, filters: [{ name: 'Executável', extensions: ['exe'] }] });
    const launcher = typeof manual === 'string' ? { name: 'Launcher personalizado', path: manual } : found[0];
    if (!launcher) { setVerifyError('Nenhum launcher foi encontrado. Selecione manualmente o executável.'); return; }
    await minecraftApi.saveConfig({ installationPath: config?.installationPath, minecraftPath: config?.minecraftPath, manifestUrl: config?.manifestUrl ?? OFFICIAL_MANIFEST_URL, launcherPath: launcher.path, launcherName: launcher.name, particles: config?.particles ?? true, reduceMotion: config?.reduceMotion ?? false }); await initialize();
  }
  const configured = Boolean(selected || config?.installationPath);
  const pending = report ? report.missing.length + report.changed.length + report.invalid.length + report.obsolete.length : undefined;
  const statusItems = [
    ['Mods válidos', report ? `${report.valid}/${report.checked}` : '—', report ? `Manifesto ${report.modpackVersion}` : 'Verifique a instalação'],
    ['Arquivos pendentes', pending === undefined ? '—' : String(pending), pending === undefined ? 'Aguardando verificação' : pending === 0 ? 'Nenhuma ação necessária' : 'Ausentes, alterados ou obsoletos'],
    ['Última verificação', lastChecked ? lastChecked.toLocaleTimeString('pt-BR', { hour: '2-digit', minute: '2-digit' }) : 'Nunca', lastChecked ? lastChecked.toLocaleDateString('pt-BR') : 'Aguardando verificação'],
    ['Integridade', report ? (pending === 0 ? 'Íntegra' : 'Atenção') : 'Aguardando', report ? (pending === 0 ? 'Todos os hashes conferem' : 'Correção necessária') : 'Nenhum arquivo foi analisado']
  ];
  return <main className="app-shell">
    <div className="ambient ambient-one"/><div className="ambient ambient-two"/>
    <header><div className="brand"><img src="/assets/nova-ordem-emblem.png"/><span>NOVA <b>ORDEM</b></span></div><button className="icon-button" onClick={() => void chooseLauncher()} aria-label="Selecionar launcher"><Settings size={19}/></button></header>
    <section className="hero">
      <div className="hero-art"/><div className="hero-copy"><span className="eyebrow"><Sparkles size={14}/> SERVIDOR PRIVADO</span><h1>Seu mundo está<br/><em>esperando.</em></h1><p>Minecraft Java Edition <strong>1.20.1</strong> <i/> Forge <strong>47.4.0</strong></p>
      <div className="install-status"><ShieldCheck size={18}/><div><small>STATUS DA INSTALAÇÃO</small><b>{configured ? 'Instalação selecionada' : 'Configuração necessária'}</b></div></div>
      <button className="primary" onClick={() => configured ? void verify() : setSetupOpen(true)} disabled={loading || checking}>{configured ? <><Compass size={20}/>{checking ? <><span className="loading-dot"/> {updating ? 'Atualizando modpack…' : 'Verificando arquivos…'}</> : ' Verificar arquivos'}</> : <><FolderSearch size={20}/> Configurar instalação</>}</button></div>
    </section>
    <section className="content"><div className="section-head"><div><span className="eyebrow">CENTRO DE CONTROLE</span><h2>Pronto para a próxima aventura</h2></div><button className="quiet" onClick={() => setSetupOpen(true)}><Settings size={16}/> Gerenciar instalação</button></div>
      <div className="cards">{statusItems.map(([title, value, detail]) => <article className="stat" key={title}><span>{title}</span><strong>{value}</strong><small>{detail}</small></article>)}</div>
      {error && <p className="notice">{error} Use “Gerenciar instalação” para tentar novamente.</p>}
      {verifyError && <p className="notice">{verifyError}</p>}
      {report && <p className="notice success">Manifesto {report.modpackVersion}: {report.valid}/{report.checked} válidos · {report.missing.length} ausentes · {report.changed.length + report.invalid.length} alterados · {report.obsolete.length} versões antigas.</p>}
      {update && <p className="notice success">Atualização {update.modpackVersion}: {update.downloaded.length} arquivos baixados e {update.updated.length} substituídos · {update.archived.length} versões antigas guardadas em backup.</p>}
      {report && pending === 0 && <button className="quiet" onClick={() => void openLauncher()}><Compass size={16}/> Abrir {config?.launcherName ?? 'launcher'}</button>}
    </section>
    {setupOpen && <div className="dialog-backdrop" role="presentation"><section className="dialog" role="dialog" aria-modal="true" aria-label="Selecionar instalação"><button className="close" onClick={() => setSetupOpen(false)}>×</button><span className="eyebrow">PRIMEIRA CONFIGURAÇÃO</span><h2>Onde está seu modpack?</h2><p>Encontramos instalações dentro da sua pasta do Minecraft. Escolha apenas a pasta do perfil que deseja gerenciar — nenhum arquivo será modificado agora.</p><div className="install-list">{installations.length ? installations.map(item => <button key={item.path} onClick={() => void choose(item)} disabled={!item.valid || saving}><FolderSearch size={18}/><span><b>{item.name}</b><small>{item.path}</small></span>{item.valid ? <ShieldCheck size={18}/> : <small>Inválida</small>}</button>) : <div className="empty">Nenhuma instalação válida foi encontrada em <code>%APPDATA%\.minecraft\versions</code>.</div>}</div><div className="dialog-foot"><span>Detectamos apenas diretórios com um arquivo <code>.json</code> de versão.</span><button className="quiet" onClick={() => void initialize()}><Download size={16}/> Procurar novamente</button></div></section></div>}
  </main>;
}
