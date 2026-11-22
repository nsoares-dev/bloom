# Bloom no macOS — Etapa 1: fundação — plano de implementação

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** O Bloom compila e abre no Mac (Ilha no topo, Dock embaixo, menu na barra de menus), com todas as funções que a interface chama respondendo, sem mudar o comportamento no Windows.

**Architecture:** `main.rs` vira um despachante: os módulos que usam Windows ficam atrás de `#[cfg(windows)]`, o corpo atual de `main()` vai sem alterações para `win_main.rs`, e nasce `mac/` com o próprio `run()` e o conjunto de comandos Mac com os **mesmos nomes**. Um teste de paridade garante que Mac e Windows registram exatamente os mesmos comandos. Nesta etapa os comandos Mac são provisórios; as etapas 2–6 trocam cada um pela implementação real.

**Tech Stack:** Rust stable (rustup), Tauri 2.11, React/Vite (sem mudanças nesta etapa), bun, GitHub Actions.

**Spec:** `docs/superpowers/specs/2026-11-22-macos-port-design.md`

**Escopo:** este plano cobre só a etapa 1 da "Ordem sugerida" do spec. As etapas 2–6 (janelas e notch; Dock; mídia; controles e status; aparelhos/prateleira/alertas/permissões) ganham um plano cada, escrito depois desta etapa, porque dependem da estrutura que ela cria.

**Ajuste ao spec (decisão do construtor, permitida em "Divisão fina … deixado para o construtor"):** em vez de dividir `commands.rs`/`services.rs` por área já agora, sem Windows para testar, o Mac ganha um conjunto paralelo de comandos em `mac/`. Lógica que não depende de sistema (configurações, apps fixados, i18n, onboarding) é compartilhada quando um módulo compila no Mac sem edição; quando não compila, é extraída para um módulo comum na etapa que precisar dela.

## Global Constraints

- Máquina: MacBook Air 13" (Mac17,3), arm64, macOS 26.6.
- Tauri `2.11`, `@tauri-apps/cli ~2.11.5` (versões do repo, não atualizar).
- Comandos Mac com o mesmo nome, parâmetros e tipo de retorno dos comandos Windows.
- Comando sem implementação no Mac: leituras devolvem o valor vazio/padrão do tipo de retorno; ações devolvem `Err("not yet implemented on macOS: <nome>")`. Nunca fingir sucesso de uma ação.
- O código Windows só muda de forma mecânica (atributos `cfg`, mover arquivo). Nenhuma lógica Windows é editada.
- Toda etapa só está pronta com a CI do Windows **e** a do Mac verdes no fork.
- Repositório: fork de `little-systems/bloom` na conta do Nicolas, branch `macos-port`.

## Review Focus

1. **Comando chamado pela interface que o Mac não registra** → a promessa rejeita com "command not found" e a tela pode ficar em branco. Coberto pelo teste de paridade (Task 3) e pelo teste "interface ⊆ registrados" (Task 3).
2. **Leitura provisória devolvendo erro em vez de valor padrão** → a interface trava na inicialização como no teste do navegador. Coberto pela verificação manual da Task 4 (console sem erros de `get_*`/`load_*`).
3. **Compilação Windows quebrada por um `cfg` esquecido** → só aparece na CI. Coberto pelo job Windows (Task 1) rodando em cada push.
4. **Segunda instância aberta no Mac** → duas Ilhas sobrepostas. Coberto na Task 4 (plugin single-instance).
5. **Ilha/Dock em posição errada em tela Retina** (coordenadas lógicas vs. físicas, 2560×1664 físico = 1280×832 lógico) → janelas fora da tela. Coberto na Task 4 (verificação por captura de tela).

---

### Task 1: Ferramentas, fork e CI nas duas plataformas

**Files:**
- Modify: `.github/workflows/ci.yml`

**Interfaces:**
- Produces: job `build` (windows-latest, inalterado) e job `build-macos` (macos-latest), disparados também por push em `macos-port`.

- [ ] **Step 1: Instalar ferramentas**

Run:
```bash
brew install gh oven-sh/bun/bun
```
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
```
Expected: `rustc --version`, `cargo --version`, `bun --version`, `gh --version` imprimem versões.

- [ ] **Step 2: Login no GitHub (o Nicolas faz)**

O Nicolas roda `gh auth login` no terminal. Expected: `gh auth status` mostra a conta logada.

- [ ] **Step 3: Criar o fork e a branch**

Run: `gh repo fork little-systems/bloom --remote --remote-name origin` dentro de `~/Documents/GitHub/bloom` (o clone atual vira `upstream`), depois `git switch -c macos-port`.
Expected: `git remote -v` mostra `origin` = fork do Nicolas e `upstream` = little-systems.

- [ ] **Step 4: Ajustar a CI**

Em `ci.yml`: `on.push.branches: [main, macos-port]` e `on.pull_request.branches: [main, macos-port]`. Adicionar job `build-macos` (`runs-on: macos-latest`) copiando os passos do job `build` de "checkout" até "Clippy", acrescido de `cargo test --locked` em `src-tauri`. Os passos de i18n e triage podem ficar só no job Windows.

- [ ] **Step 5: Commit e push**

```bash
git add .github/workflows/ci.yml docs/superpowers
git commit -m "ci: build on macOS and on the macos-port branch"
git push -u origin macos-port
```
Expected: no GitHub Actions do fork, job `build` verde, `build-macos` vermelho (o Rust ainda não compila no Mac). Isso é esperado até a Task 2.

---

### Task 2: Compilar no Mac (dependências por plataforma e `main` despachante)

**Files:**
- Modify: `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`
- Move: `src-tauri/src/main.rs` → `src-tauri/src/win_main.rs` (com `git mv`)
- Create: `src-tauri/src/main.rs`, `src-tauri/src/mac/mod.rs`

**Interfaces:**
- Produces: `win_main::run()` (o antigo `main()` intacto, renomeado e `pub`); `mac::run()`; o `main()` novo chama um ou outro por `cfg`.

- [ ] **Step 1: Ver a falha atual**

Run: `cd src-tauri && cargo check`
Expected: FAIL no crate `windows`/`webview2-com`/`wmi` ou em `use windows::…`.

- [ ] **Step 2: Dependências por alvo**

Em `Cargo.toml`, mover `windows-core`, `windows-collections`, `windows`, `wmi` e `webview2-com` para `[target.'cfg(windows)'.dependencies]`, sem mudar versões nem features. As demais continuam em `[dependencies]`.

- [ ] **Step 3: Separar o `main`**

`git mv src-tauri/src/main.rs src-tauri/src/win_main.rs`. Em `win_main.rs`: remover só as linhas `mod …;` e o `#![cfg_attr…]` (vão para o novo `main.rs`) e trocar `fn main()` por `pub fn run()`. Nada mais muda.

Novo `main.rs`: o `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]`; as mesmas declarações `mod`; cada módulo que **não compila no Mac sem edição** recebe `#[cfg(windows)]`, e os que compilam ficam sem atributo (candidatos pelo levantamento: `diagnostics`, `i18n`, `settings_window`, `types`, `updater`; confirmar com `cargo check`); `#[cfg(windows)] mod win_main;` e `#[cfg(target_os = "macos")] mod mac;`; `fn main()` chamando `win_main::run()` ou `mac::run()` conforme o `cfg`.

- [ ] **Step 4: `mac::run()` mínimo**

Em `mac/mod.rs`: `pub fn run()` com `tauri::Builder::default()`, os mesmos plugins do Windows (opener, dialog, clipboard, drag, autostart com `MacosLauncher::LaunchAgent`; **sem** updater) e `.run(tauri::generate_context!())`. Ainda sem comandos.

- [ ] **Step 5: Verificar no Mac**

Run: `cd src-tauri && cargo check && cargo clippy --all-targets && cargo test`
Expected: PASS. Os testes que rodam são os dos módulos compartilhados.

- [ ] **Step 6: Conferir que o Windows não mudou**

Run: `git diff --cached -M --stat` e `git diff --cached -M src-tauri/src/win_main.rs`
Expected: `main.rs → win_main.rs` aparece como renomeação, e o diff mostra só as linhas `mod`/`cfg_attr` removidas e `fn main()` → `pub fn run()`.

- [ ] **Step 7: Commit e push**

```bash
git add -A src-tauri
git commit -m "build: compile on macOS with per-platform entry points"
git push
```
Expected: CI `build` (Windows) e `build-macos` verdes.

---

### Task 3: Todos os comandos registrados no Mac, com teste de paridade

**Files:**
- Create: `src-tauri/src/mac/commands.rs`, `src-tauri/src/mac/parity_tests.rs`
- Modify: `src-tauri/src/mac/mod.rs`

**Interfaces:**
- Consumes: a lista `generate_handler![…]` de `win_main.rs` (117 comandos) e as assinaturas em `commands.rs`, `wifi.rs`, `bluetooth.rs`, `battery.rs`, `dock_extras.rs`, `jack.rs`, `usb.rs`, `onboarding.rs`, `agent_alerts.rs`, `window_region.rs`, `i18n.rs`, `updater.rs`.
- Produces: `mac::commands::<mesmo_nome>` para cada comando Windows que não vem de um módulo compartilhado; o `generate_handler!` de `mac::run()` com o mesmo conjunto de nomes (mesmo nome final de função; o caminho pode ser `commands::x` ou `i18n::x`).

- [ ] **Step 1: Escrever os testes que falham**

Em `parity_tests.rs` (`#[cfg(test)]`), dois testes que leem fontes com `include_str!`:

```rust
#[test]
fn mac_registers_same_commands_as_windows() {
    let win = handler_names(include_str!("../win_main.rs"));
    let mac = handler_names(include_str!("mod.rs"));
    assert_eq!(win, mac); // BTreeSet<String> dos nomes finais (após o último `::`)
}

#[test]
fn every_frontend_invoke_is_registered_on_mac() {
    let mac = handler_names(include_str!("mod.rs"));
    for name in frontend_invokes() { // varre ../../src/**/*.{ts,tsx} por invoke("nome" / invoke<T>("nome"
        assert!(mac.contains(&name), "frontend calls `{name}`, not registered on macOS");
    }
}
```
`handler_names(src: &str) -> BTreeSet<String>` extrai o bloco entre `generate_handler![` e `]` e pega o último segmento de cada caminho. `frontend_invokes()` usa `std::fs` a partir de `env!("CARGO_MANIFEST_DIR")/../src`.

- [ ] **Step 2: Rodar e ver falhar**

Run: `cd src-tauri && cargo test parity`
Expected: FAIL. Os conjuntos diferem (o Mac ainda não tem comandos).

- [ ] **Step 3: Comandos provisórios**

Em `mac/commands.rs`, um `#[tauri::command]` para cada comando Windows que não vem de módulo compartilhado. Usar **a mesma assinatura** (nome, parâmetros com os mesmos nomes e tipos, mesmo tipo de retorno; `async` se o original for), seguindo a regra das Global Constraints: leitura → valor padrão (`Default::default()`, lista vazia, `false`, `0`); ação → `Err(...)` se o tipo de retorno for `Result`; senão, registrar com `diagnostics::record("unsupported", "<nome>")` e retornar `()`. Exceções reais já nesta etapa (compartilhar a lógica existente se o módulo compilar, senão implementar com `std::fs` + `serde_json` no mesmo arquivo de configurações que o Windows usa): `load_settings`, `save_setting`, `settings_file_readable`, `export_settings_to_file`, `import_settings_from_file`, `reset_settings`, `save_pinned_apps`, `load_pinned_apps`, `quit_bloom`, `restart_bloom`, `open_settings_window`, `close_settings_window`. Tipos de `types.rs` são reaproveitados.

- [ ] **Step 4: Registrar e rodar os testes**

Registrar todos em `mac::run()` com `generate_handler!`. Run: `cd src-tauri && cargo test && cargo clippy --all-targets`
Expected: PASS, incluindo os dois testes de paridade.

- [ ] **Step 5: Commit e push**

```bash
git add src-tauri/src/mac
git commit -m "feat(macos): register every command with provisional implementations"
git push
```
Expected: CI verde nas duas plataformas.

---

### Task 4: O app abre no Mac

**Files:**
- Modify: `src-tauri/src/mac/mod.rs`, `src-tauri/Cargo.toml` (plugin single-instance só para macOS)

**Interfaces:**
- Consumes: `mac::commands::open_settings_window` (Task 3).
- Produces: `mac::run()` que posiciona as janelas, mostra o menu da barra de menus e impede segunda instância.

- [ ] **Step 1: Uma instância só**

Adicionar `tauri-plugin-single-instance = "2"` em `[target.'cfg(target_os = "macos")'.dependencies]` e registrar no `mac::run()` com callback que chama `open_settings_window` (mesmo comportamento do Windows).

- [ ] **Step 2: Posicionar as janelas no `setup`**

Pegar o monitor principal (`app.primary_monitor()`), em coordenadas lógicas: `main` com largura da tela, x=0, y=0, altura 420 (valor do `tauri.conf.json`); `dock` com largura da tela, encostada embaixo, altura do `tauri.conf.json`; `overlay` do tamanho da tela. Mostrar `main` e `dock`. Clique atravessando a janela e o notch ficam para a etapa 2.

- [ ] **Step 3: Menu na barra de menus**

Mesmo menu do Windows (Abrir Configurações, Reiniciar, Sair) via `TrayIconBuilder`, com `i18n::TrayMessages` e `i18n::set_tray_language`. "Sair" chama `app.exit(0)`.

- [ ] **Step 4: Rodar e verificar**

Run: `bun install --frozen-lockfile && bun run tauri dev` (em background).
Expected, verificado com captura de tela e o console do webview:
- A Ilha aparece no topo, no centro, e a Dock embaixo, ambas dentro da tela.
- O ícone do Bloom aparece na barra de menus, e "Abrir Configurações" abre a janela de configurações.
- O console não mostra "command not found", nem erros vindos de comandos `get_*`/`load_*`.
- Rodar `bun run tauri dev` uma segunda vez não abre outra Ilha.
- "Sair" fecha tudo.

- [ ] **Step 5: Commit e push**

```bash
git add src-tauri
git commit -m "feat(macos): open island, dock and menu bar icon"
git push
```
Expected: CI verde nas duas plataformas. Fim da etapa 1. Próximo passo: escrever o plano da etapa 2 (janelas, cliques e notch).
