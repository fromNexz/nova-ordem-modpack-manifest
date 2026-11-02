# Nova Ordem — Modpack Manager 0.2.0

Aplicativo Windows (Tauri 2 + React) para verificar e sincronizar o modpack oficial.

## Utilização

Instale o executável Windows da release `modpack-1.1.4`. Feche o Minecraft, selecione o perfil e clique em **Verificar arquivos**. O aplicativo baixa arquivos ausentes/alterados, confere tamanho e SHA-256, e depois move mods obsoletos explicitamente listados para `backups/novaordem-manager/` dentro do perfil. Mods adicionais fora dessa lista são preservados.

Substituições no mesmo caminho mantêm o backup local já existente (`.nome.jar.novaordem-backup-N`). O manifesto schema 2 exige esta versão; aplicativos anteriores recusam esse manifesto para não instalar versões duplicadas.

O aplicativo não instala Forge, não atualiza o servidor e não testa compatibilidade de execução. O perfil precisa de Minecraft 1.20.1 / Forge 47.4.0 e de servidor compatível.

## Desenvolvimento

```powershell
npm ci
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
npm run package:windows
```

O instalador é criado em `src-tauri/target/release/bundle/nsis/`. As configurações do usuário ficam em `%APPDATA%/NovaOrdem/settings.json` e não são distribuídas.

O backend suporta schema 1 e 2 (`removeFiles`, lista explícita de JARs obsoletos). Rejeita remoções fora de `mods/`, subdiretórios, conflitos com arquivos ativos e duplicatas. Resolve caminhos para recusar links que escapem do perfil. Uma falha na movimentação tenta restaurar os arquivos já movidos.
