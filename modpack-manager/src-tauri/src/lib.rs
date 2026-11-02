use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs, io::{Read, Write}, path::{Component, Path, PathBuf}, process::Command, thread, time::Duration};
use url::Url;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct MinecraftInstallation { name: String, path: String, valid: bool, reason: Option<String> }

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
#[serde(default)]
struct AppConfig { installation_path: Option<String>, minecraft_path: Option<String>, manifest_url: Option<String>, launcher_path: Option<String>, launcher_name: Option<String>, reduce_motion: bool, particles: bool }
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Launcher { name: String, path: String }

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoteManifest { schema_version: u32, modpack: ManifestModpack, files: Vec<ManifestFile>, #[serde(default)] remove_files: Vec<String> }
#[derive(Debug, Deserialize)]
struct ManifestModpack { version: String }
#[derive(Debug, Deserialize)]
struct ManifestFile { path: String, sha256: String, size: u64, url: String, required: bool }
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct VerificationReport { modpack_version: String, checked: usize, valid: usize, missing: Vec<String>, changed: Vec<String>, invalid: Vec<String>, obsolete: Vec<String> }
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateReport { modpack_version: String, downloaded: Vec<String>, updated: Vec<String>, already_valid: usize, archived: Vec<String> }

const OFFICIAL_MANIFEST_URL: &str = "https://raw.githubusercontent.com/fromNexz/nova-ordem-modpack-manifest/main/manifest.json";
impl Default for AppConfig { fn default() -> Self { Self { installation_path: None, minecraft_path: minecraft_root().map(display), manifest_url: Some(OFFICIAL_MANIFEST_URL.into()), launcher_path: None, launcher_name: None, reduce_motion: false, particles: true } } }
fn display(path: PathBuf) -> String { path.to_string_lossy().into_owned() }
fn minecraft_root() -> Option<PathBuf> { dirs::data_dir().map(|p| p.join(".minecraft")) }
fn config_path() -> Result<PathBuf, String> {
  let root = dirs::config_dir().ok_or("Não foi possível localizar o diretório de configuração do Windows.")?.join("NovaOrdem");
  fs::create_dir_all(&root).map_err(|e| format!("Não foi possível criar configuração: {e}"))?;
  Ok(root.join("settings.json"))
}
fn inspect(path: &Path) -> MinecraftInstallation {
  let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("Instalação").to_owned();
  let json = path.join(format!("{name}.json"));
  let jar = path.join(format!("{name}.jar"));
  let valid = path.is_dir() && json.is_file();
  MinecraftInstallation { name, path: path.to_string_lossy().into_owned(), valid, reason: (!valid).then(|| if jar.is_file() { "Arquivo de versão .json ausente".into() } else { "Não parece ser uma instalação Minecraft".into() }) }
}
fn validate_https_url(value: &str) -> Result<Url, String> {
  let url = Url::parse(value).map_err(|_| "A URL do manifesto é inválida.")?;
  if url.scheme() != "https" || url.host_str().is_none() { return Err("Use uma URL HTTPS válida para o manifesto.".into()); }
  Ok(url)
}
fn managed_relative_path(raw: &str) -> Result<PathBuf, String> {
  let path = Path::new(raw);
  if raw.is_empty() || path.is_absolute() || raw.contains('\\') || path.components().any(|c| matches!(c, Component::ParentDir | Component::RootDir | Component::Prefix(_))) { return Err(format!("Caminho inseguro no manifesto: {raw}")); }
  let first = path.components().next().and_then(|c| if let Component::Normal(v) = c { v.to_str() } else { None });
  match first { Some("mods" | "config" | "defaultconfigs" | "kubejs" | "resourcepacks" | "shaderpacks") => Ok(path.to_path_buf()), _ => Err(format!("Caminho não gerenciado no manifesto: {raw}")) }
}
fn hash_file(path: &Path) -> Result<String, String> {
  let mut file = fs::File::open(path).map_err(|e| format!("Não foi possível ler {}: {e}", path.display()))?;
  let mut hasher = Sha256::new(); let mut buffer = [0_u8; 64 * 1024];
  loop { let read = file.read(&mut buffer).map_err(|e| e.to_string())?; if read == 0 { break; } hasher.update(&buffer[..read]); }
  Ok(format!("{:x}", hasher.finalize()))
}
fn fetch_manifest() -> Result<RemoteManifest, String> {
  let config = get_config()?;
  let manifest_url = config.manifest_url.ok_or("Configure a URL HTTPS do manifesto oficial antes de verificar.")?;
  validate_https_url(&manifest_url)?;
  let response = reqwest::blocking::Client::builder().timeout(std::time::Duration::from_secs(30)).build().map_err(|e| e.to_string())?.get(&manifest_url).send().map_err(|e| format!("Falha ao buscar manifesto: {e}"))?;
  if !response.status().is_success() { return Err(format!("O servidor respondeu com HTTP {}.", response.status())); }
  let body = response.text().map_err(|e| format!("Não foi possível ler o manifesto: {e}"))?;
  let manifest: RemoteManifest = serde_json::from_str(body.trim_start_matches('\u{feff}')).map_err(|e| format!("Manifesto JSON inválido: {e}"))?;
  validate_manifest(&manifest)?;
  Ok(manifest)
}
fn file_matches(path: &Path, entry: &ManifestFile) -> Result<bool, String> {
  if !path.is_file() { return Ok(false); }
  if fs::metadata(path).map_err(|e| e.to_string())?.len() != entry.size { return Ok(false); }
  Ok(hash_file(path)?.eq_ignore_ascii_case(&entry.sha256))
}
fn temporary_path(target: &Path) -> Result<PathBuf, String> {
  let name = target.file_name().and_then(|value| value.to_str()).ok_or("Nome de arquivo inválido.")?;
  Ok(target.with_file_name(format!(".{name}.novaordem-download-{}.part", std::process::id())))
}
fn backup_path(target: &Path) -> Result<PathBuf, String> {
  let name = target.file_name().and_then(|value| value.to_str()).ok_or("Nome de arquivo inválido.")?;
  for suffix in 0..1000 { let candidate = target.with_file_name(format!(".{name}.novaordem-backup-{suffix}")); if !candidate.exists() { return Ok(candidate); } }
  Err("Há muitos arquivos de backup para este mod.".into())
}
fn download_and_replace(entry: &ManifestFile, target: &Path) -> Result<bool, String> {
  let source = validate_https_url(&entry.url)?;
  let parent = target.parent().ok_or("Destino de arquivo inválido.")?;
  fs::create_dir_all(parent).map_err(|e| format!("Não foi possível criar pasta de destino: {e}"))?;
  let temp = temporary_path(target)?;
  if temp.exists() { fs::remove_file(&temp).map_err(|e| format!("Não foi possível limpar download anterior: {e}"))?; }
  let result = (|| -> Result<(), String> {
    let mut response = reqwest::blocking::Client::builder().timeout(std::time::Duration::from_secs(120)).build().map_err(|e| e.to_string())?.get(source).send().map_err(|e| format!("Falha ao baixar {}: {e}", entry.path))?;
    if !response.status().is_success() { return Err(format!("Download de {} falhou com HTTP {}.", entry.path, response.status())); }
    let mut output = fs::OpenOptions::new().write(true).create_new(true).open(&temp).map_err(|e| format!("Não foi possível preparar download: {e}"))?;
    let mut hasher = Sha256::new(); let mut total = 0_u64; let mut buffer = [0_u8; 64 * 1024];
    loop { let read = response.read(&mut buffer).map_err(|e| format!("Falha durante download: {e}"))?; if read == 0 { break; } output.write_all(&buffer[..read]).map_err(|e| format!("Falha ao gravar download: {e}"))?; hasher.update(&buffer[..read]); total += read as u64; }
    output.flush().map_err(|e| e.to_string())?;
    if total != entry.size { return Err(format!("Tamanho inválido ao baixar {}.", entry.path)); }
    if !format!("{:x}", hasher.finalize()).eq_ignore_ascii_case(&entry.sha256) { return Err(format!("Hash inválido ao baixar {}.", entry.path)); }
    Ok(())
  })();
  if let Err(error) = result { let _ = fs::remove_file(&temp); return Err(error); }
  if !target.exists() { fs::rename(&temp, target).map_err(|e| format!("Não foi possível concluir download: {e}"))?; return Ok(false); }
  let backup = backup_path(target)?;
  fs::rename(target, &backup).map_err(|e| format!("Não foi possível preservar arquivo anterior: {e}"))?;
  if let Err(error) = fs::rename(&temp, target) { let _ = fs::rename(&backup, target); let _ = fs::remove_file(&temp); return Err(format!("Não foi possível concluir atualização: {error}")); }
  Ok(true)
}
#[tauri::command]
fn detected_launchers() -> Vec<Launcher> {
  let mut candidates: Vec<(String, PathBuf)> = Vec::new();
  if let Some(local) = dirs::data_local_dir() { candidates.push(("Minecraft Launcher oficial".into(), local.join("Microsoft/WindowsApps/MinecraftLauncher.exe"))); }
  if let Some(roaming) = dirs::config_dir() { candidates.push(("TLauncher".into(), roaming.join(".minecraft/TLauncher.exe"))); candidates.push(("TLauncher".into(), roaming.join(".tlauncher/TLauncher.exe"))); }
  candidates.push(("Minecraft Launcher oficial".into(), PathBuf::from(r"C:\Program Files (x86)\Minecraft Launcher\MinecraftLauncher.exe")));
  candidates.into_iter().filter(|(_, p)| p.is_file()).map(|(name, path)| Launcher { name, path: display(path) }).collect()
}
#[tauri::command]
fn detect_launchers() -> Vec<Launcher> { detected_launchers() }
fn validate_manifest(manifest: &RemoteManifest) -> Result<(), String> {
  if !matches!(manifest.schema_version, 1 | 2) { return Err("Versão de manifesto não suportada. Atualize o aplicativo.".into()); }
  if manifest.modpack.version.trim().is_empty() || manifest.files.len() > 20_000 { return Err("Manifesto inválido ou grande demais.".into()); }
  for file in &manifest.files { managed_relative_path(&file.path)?; validate_https_url(&file.url)?; if file.sha256.len() != 64 || !file.sha256.bytes().all(|b| b.is_ascii_hexdigit()) { return Err(format!("SHA-256 inválido: {}", file.path)); } }
  let mut paths = std::collections::HashSet::new();
  for file in &manifest.files { if !paths.insert(file.path.to_ascii_lowercase()) { return Err("Caminhos duplicados no manifesto.".into()); } }
  if manifest.remove_files.len() > 20_000 { return Err("Lista de substituições muito grande.".into()); }
  for old in &manifest.remove_files {
    obsolete_path(old)?;
    if !paths.insert(old.to_ascii_lowercase()) { return Err(format!("Remoção duplicada ou conflitante: {old}")); }
  }
  Ok(())
}

// Only explicitly retired JARs can be archived. User-added mods are untouched.
fn obsolete_path(raw: &str) -> Result<PathBuf, String> {
  let p = managed_relative_path(raw)?;
  if !raw.starts_with("mods/") || raw.split('/').count() != 2 || !raw.to_ascii_lowercase().ends_with(".jar") || raw.contains(':') {
    return Err(format!("Substituição inválida: {raw}"));
  }
  Ok(p)
}
fn obsolete_files(root: &Path, manifest: &RemoteManifest) -> Result<Vec<String>, String> {
  manifest.remove_files.iter().filter_map(|raw| match obsolete_path(raw) {
    Ok(p) if root.join(&p).exists() => Some(Ok(raw.clone())), Ok(_) => None, Err(e) => Some(Err(e))
  }).collect()
}
fn archive_obsolete(root: &Path, manifest: &RemoteManifest) -> Result<Vec<String>, String> {
  let pending = obsolete_files(root, manifest)?;
  if pending.is_empty() { return Ok(pending); }
  let root = fs::canonicalize(root).map_err(|e| e.to_string())?;
  // Resolve every source before moving any; refuse links escaping the profile.
  for raw in &pending {
    let source = fs::canonicalize(root.join(raw)).map_err(|e| e.to_string())?;
    if !source.starts_with(&root) || !source.is_file() { return Err(format!("Mod fora da instalação: {raw}")); }
  }
  let backups = root.join("backups/novaordem-manager");
  fs::create_dir_all(&backups).map_err(|e| e.to_string())?;
  let backups = fs::canonicalize(backups).map_err(|e| e.to_string())?;
  if !backups.starts_with(&root) { return Err("Pasta de backup fora da instalação.".into()); }
  let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e| e.to_string())?.as_nanos();
  let batch = backups.join(format!("{stamp}-{}", std::process::id()));
  fs::create_dir(&batch).map_err(|e| e.to_string())?;
  let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();
  for raw in &pending {
    let source = root.join(raw);
    let destination = batch.join(source.file_name().ok_or("Nome inválido")?);
    if let Err(error) = fs::rename(&source, &destination) {
      let mut rollback_errors = Vec::new();
      for (old, backup) in moved.iter().rev() { if let Err(e) = fs::rename(backup, old) { rollback_errors.push(e.to_string()); } }
      return Err(format!("Não foi possível arquivar {raw}: {error}. Backup: {}. Restauração: {:?}", batch.display(), rollback_errors));
    }
    moved.push((source, destination));
  }
  Ok(pending)
}
#[tauri::command]
fn detect_minecraft_installations() -> Result<Vec<MinecraftInstallation>, String> {
  let versions = minecraft_root().ok_or("A pasta AppData não foi localizada.")?.join("versions");
  let entries = fs::read_dir(&versions).map_err(|_| format!("Não foi possível ler {}", versions.display()))?;
  Ok(entries.filter_map(Result::ok).map(|e| e.path()).filter(|p| p.is_dir()).map(|p| inspect(&p)).filter(|i| i.valid).collect())
}
#[tauri::command]
fn validate_installation(path: String) -> MinecraftInstallation { inspect(Path::new(&path)) }
#[tauri::command]
fn get_config() -> Result<AppConfig, String> {
  let path = config_path()?;
  if !path.exists() { return Ok(AppConfig::default()); }
  let content = fs::read_to_string(path).map_err(|e| format!("Não foi possível ler configuração: {e}"))?;
  serde_json::from_str(&content).map_err(|e| format!("Configuração inválida: {e}"))
}
#[tauri::command]
fn save_config(config: AppConfig) -> Result<(), String> {
  if let Some(ref installation) = config.installation_path { if !inspect(Path::new(installation)).valid { return Err("A instalação escolhida não possui um arquivo .json de versão válido.".into()); } }
  if let Some(ref url) = config.manifest_url { validate_https_url(url)?; }
  if let Some(ref launcher) = config.launcher_path { let path = Path::new(launcher); if !path.is_file() || path.extension().and_then(|e| e.to_str()).map(|e| !e.eq_ignore_ascii_case("exe")).unwrap_or(true) { return Err("O launcher selecionado não é um executável válido.".into()); } }
  let content = serde_json::to_vec_pretty(&config).map_err(|e| e.to_string())?;
  fs::write(config_path()?, content).map_err(|e| format!("Não foi possível salvar configuração: {e}"))
}
#[tauri::command]
fn launch_selected_launcher(app: tauri::AppHandle) -> Result<Launcher, String> {
  let mut config = get_config()?;
  let selected = config.launcher_path.as_ref().filter(|path| Path::new(path).is_file()).map(|path| Launcher { name: config.launcher_name.clone().unwrap_or_else(|| "Launcher selecionado".into()), path: path.clone() });
  let launcher = selected.or_else(|| detected_launchers().into_iter().next()).ok_or("Nenhum launcher foi encontrado. Use o botão de engrenagem para escolher o executável.")?;
  if config.launcher_path.as_deref() != Some(&launcher.path) {
    config.launcher_path = Some(launcher.path.clone()); config.launcher_name = Some(launcher.name.clone()); save_config(config)?;
  }
  let executable = PathBuf::from(&launcher.path);
  let folder = executable.parent().ok_or("Pasta do launcher inválida.")?;
  Command::new(&executable).current_dir(folder).spawn().map_err(|e| format!("Não foi possível abrir {}: {e}", launcher.name))?;
  let app_to_close = app.clone();
  thread::spawn(move || { thread::sleep(Duration::from_millis(750)); app_to_close.exit(0); });
  Ok(launcher)
}
#[tauri::command]
fn verify_manifest() -> Result<VerificationReport, String> {
  let config = get_config()?;
  let install = config.installation_path.ok_or("Selecione uma instalação antes de verificar.")?;
  let manifest = fetch_manifest()?;
  let root = PathBuf::from(install); let mut valid = 0; let mut missing = Vec::new(); let mut changed = Vec::new(); let mut invalid = Vec::new();
  for entry in &manifest.files { let relative = managed_relative_path(&entry.path)?; let local = root.join(relative); if !local.is_file() { if entry.required { missing.push(entry.path.clone()); } continue; } let metadata = fs::metadata(&local).map_err(|e| e.to_string())?; if metadata.len() != entry.size { changed.push(entry.path.clone()); continue; } match hash_file(&local) { Ok(hash) if hash.eq_ignore_ascii_case(&entry.sha256) => valid += 1, Ok(_) => changed.push(entry.path.clone()), Err(_) => invalid.push(entry.path.clone()) }; }
  let obsolete = obsolete_files(&root, &manifest)?;
  Ok(VerificationReport { modpack_version: manifest.modpack.version, checked: manifest.files.len(), valid, missing, changed, invalid, obsolete })
}
#[tauri::command]
fn sync_modpack() -> Result<UpdateReport, String> {
  let config = get_config()?;
  let install = PathBuf::from(config.installation_path.ok_or("Selecione uma instalação antes de atualizar.")?);
  let manifest = fetch_manifest()?;
  let mut downloaded = Vec::new(); let mut updated = Vec::new(); let mut already_valid = 0;
  for entry in &manifest.files {
    let relative = managed_relative_path(&entry.path)?; let target = install.join(relative);
    if file_matches(&target, entry)? { already_valid += 1; continue; }
    if !entry.required { continue; }
    if download_and_replace(entry, &target)? { updated.push(entry.path.clone()); } else { downloaded.push(entry.path.clone()); }
  }
  // Remove retired versions only after all required replacements passed SHA-256.
  let archived = archive_obsolete(&install, &manifest)?;
  Ok(UpdateReport { modpack_version: manifest.modpack.version, downloaded, updated, already_valid, archived })
}
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() { tauri::Builder::default().plugin(tauri_plugin_dialog::init()).invoke_handler(tauri::generate_handler![detect_minecraft_installations, validate_installation, get_config, save_config, verify_manifest, sync_modpack, detect_launchers, launch_selected_launcher]).run(tauri::generate_context!()).expect("erro ao executar Nova Ordem"); }

#[cfg(test)]
mod update_tests {
  use super::*;
  fn manifest() -> RemoteManifest {
    RemoteManifest { schema_version: 2, modpack: ManifestModpack { version: "1.1.4".into() },
      files: vec![ManifestFile { path: "mods/current.jar".into(), sha256: "0".repeat(64), size: 3, url: "https://example.com/current.jar".into(), required: true }],
      remove_files: vec!["mods/old.jar".into()] }
  }
  #[test] fn accepts_old_schema_and_validates_cleanup() {
    let mut m = manifest(); assert!(validate_manifest(&m).is_ok());
    m.schema_version=1; assert!(validate_manifest(&m).is_ok());
    for invalid in ["../old.jar", "mods/../../old.jar", "config/settings.json", "mods/folder/x.jar", "mods/x.jar:stream", "mods/CURRENT.jar"] {
      m.remove_files=vec![invalid.into()]; assert!(validate_manifest(&m).is_err(), "{invalid}");
    }
  }
  #[test] fn preserves_unrelated_mods_and_archives_only_named_versions() {
    let root=std::env::temp_dir().join(format!("novaordem-test-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    fs::create_dir_all(root.join("mods")).unwrap();
    for name in ["old.jar","current.jar","personal.jar"] { fs::write(root.join("mods").join(name),name).unwrap(); }
    let m=manifest();
    assert_eq!(obsolete_files(&root,&m).unwrap(),vec!["mods/old.jar"]);
    assert_eq!(archive_obsolete(&root,&m).unwrap(),vec!["mods/old.jar"]);
    assert!(!root.join("mods/old.jar").exists());
    assert_eq!(fs::read_to_string(root.join("mods/personal.jar")).unwrap(),"personal.jar");
    assert!(root.join("mods/current.jar").is_file());
    let batch=fs::read_dir(root.join("backups/novaordem-manager")).unwrap().next().unwrap().unwrap().path();
    assert_eq!(fs::read_to_string(batch.join("old.jar")).unwrap(),"old.jar");
    assert!(archive_obsolete(&root,&m).unwrap().is_empty());
    fs::remove_dir_all(&root).unwrap();
  }
  #[test] fn rejects_case_insensitive_duplicates() {
    let mut m=manifest(); m.remove_files.push("mods/OLD.jar".into()); assert!(validate_manifest(&m).is_err());
  }
}
