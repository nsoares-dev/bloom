# Bloom no macOS — design

Data: 22/11/2026 · Máquina alvo: MacBook Air 13" (Mac17,3, Apple Silicon, tela 2560×1664 com notch), macOS 26.6.

## Intenção

Ter o **pacote todo** do Bloom rodando de verdade no Mac do Nicolas: Ilha (música com visualizador, central de controles, status, calendário/Pomodoro), Dock, popups de aparelhos conectados, prateleira de arquivos, alertas de agentes e configurações. Hoje o Bloom é só Windows: ~20 mil linhas de Rust chamando Win32/WinRT direto, sem nenhum `cfg` de plataforma; a interface (React + Tauri 2) é portátil, mas fica em branco sem o backend.

## Objetivos

- `cargo tauri dev` / `tauri build` funcionam no Mac (arm64) e o app abre com Ilha e Dock.
- As 117 funções `#[tauri::command]` existem no Mac com o **mesmo nome e mesmo formato** de entrada/saída, para a interface não precisar saber em que sistema está (exceto onde o comportamento é diferente por natureza — ver "Diferenças assumidas").
- Os eventos que o backend emite para a interface (`wired-audio-connection`, mudanças de mídia, bateria, Bluetooth etc.) também saem no Mac com o mesmo nome e conteúdo.
- O código Windows continua compilando e se comportando igual.

## Não-objetivos

- Assinar/notarizar o app e publicar release para outras pessoas. É build local para este Mac.
- Atualizador automático no Mac (o `updater.rs` baixa instalador NSIS/MSI). Fica desligado no Mac.
- Mandar o port de volta para o repositório original agora. Possível depois, mas não guia decisões.
- Intel Mac, monitores múltiplos com configurações exóticas (suporte básico: a Ilha/Dock vão na tela principal).

## Anti-objetivos (falhou mesmo que "funcione")

- A Dock da Apple ficar escondida para sempre depois que o Bloom fechar ou travar, sem jeito óbvio de voltar.
- A janela grande e transparente da Ilha/Dock engolir cliques destinados aos apps de baixo.
- Quebrar o build ou o comportamento do Windows.
- Gastar CPU/bateria à toa (o original dorme quando nada acontece; o port tem que manter isso).
- Copiar o código Windows para um fork separado (caminho B descartado).

## Abordagens consideradas

- **A. Mesmo código, uma camada por sistema — ESCOLHIDA pelo Nicolas.** Interface intacta; no Rust, o que é Windows vai para trás de `#[cfg(windows)]` e nasce uma implementação `#[cfg(target_os = "macos")]` para cada área. Comandos com os mesmos nomes. Mantém o repo sincronizável com o original.
- B. Fork só Mac (apagar Windows, reescrever). Mais simples no início, mas fica preso a uma versão antiga do Bloom.
- C. Igual ao A com as partes do Mac num helper em Swift. Descartado como base (duas linguagens, mais empacotamento); Swift só entra pontualmente se uma API não for usável a partir do Rust.

## Decisões do Nicolas

1. **Caminho A.**
2. **A Dock do Bloom substitui a Dock da Apple.** Ao iniciar (com a Dock do Bloom ligada), o Bloom guarda o estado atual de `com.apple.dock autohide` e `autohide-delay`, liga o ocultar automático com atraso enorme e reinicia a Dock. Ao sair (e ao desligar a Dock do Bloom nas configurações), restaura o que estava antes. Se travar, o próximo início detecta o estado guardado e restaura/reaplica; e o menu da bandeja tem "Restaurar Dock do macOS". Documentar o comando manual de recuperação.
3. **A Ilha nasce do notch.** Em tela com notch (`NSScreen.safeAreaInsets.top > 0` e `auxiliaryTopLeftArea/RightArea`), o estado parado da Ilha tem a largura e altura do notch, colada nele; ao expandir, cresce para os lados e para baixo a partir dele, com o miolo sempre preto. Em tela sem notch, a Ilha se comporta como no Windows (abaixo do topo, centralizada). A geometria do notch vem do backend para a interface.

4. **Código num fork na conta do Nicolas no GitHub**, branch `macos-port`. A CI existente (`windows-latest`) roda a cada etapa e é a prova de que o Windows não quebrou. Nenhuma etapa é "pronta" com a CI do Windows vermelha.
5. **Alertas de agentes entram**, mas o gancho só é instalado em `~/.claude`, Codex e OpenCode quando o Nicolas liga a opção no Bloom, e é removido ao desligar. No Mac: gancho POSIX (`sh`) no lugar de `integrations/agent-hook.ps1`, e inspeção de processos (pid pai, executável, horário de criação) via `libproc`/`sysctl` no lugar das APIs do Windows.
6. **Dock no Mac só no modo "inteligente"** (aparece quando o mouse chega embaixo, some depois). O modo "fixo" fica escondido na interface no Mac, já que o macOS não deixa reservar espaço da tela.
7. **Estados compactos da Ilha ficam nas laterais do notch**, na altura da barra de menus (mini player: capa à esquerda, visualizador/tempo à direita; timer, peeks de aparelho e alerta igualmente). Cobrem os ícones da barra de menus só enquanto ativos. O backend manda para a interface o retângulo do notch e as áreas livres à esquerda/direita (`auxiliaryTopLeftArea/RightArea`).

## Restrições

- Tauri 2.11 / wry usa **WKWebView** no Mac (não WebView2). Tudo que hoje depende de WebView2 (`webview2-com`, alvos de drop OLE em `shelf.rs`, região de janela) precisa de equivalente.
- Permissões do macOS que o usuário terá de conceder: **Acessibilidade** (listar/focar janelas, atalhos globais), **Gravação de Tela** (miniaturas de janelas da Dock), **Bluetooth**, **Captura de áudio do sistema** (visualizador; Core Audio process tap, macOS 14.4+), possivelmente **Localização** (o macOS exige para ler SSID do Wi-Fi). O onboarding deve pedir isso.
- A janela da Ilha precisa ficar acima da barra de menus e em todos os Spaces/telas cheias: nível de janela `NSStatusWindowLevel`+ e `collectionBehavior` `canJoinAllSpaces | stationary | fullScreenAuxiliary`.
- Rust ainda não está instalado na máquina; bun também não (npm funciona; `tauri.conf.json` chama `bun run dev`).
- Não há Windows disponível para teste: as mudanças no código Windows devem ser mecânicas (mover para trás de `cfg`) e são verificadas pela CI do fork (decisão 4).

## Como o Mac substitui cada área

| Área (arquivo hoje) | No Mac |
|---|---|
| Janelas, região de clique, overlay, settings (`main.rs`, `window_region.rs`, `settings_window.rs`) | `NSWindow` via Tauri + ajustes objc2 (nível, Spaces). Região de clique: `set_ignore_cursor_events` ligado/desligado conforme o mouse entra nos retângulos reportados pela interface (monitor de posição do cursor), já que não existe `SetWindowRgn`. |
| Dock: apps abertos, focar, abrir, ícones, apps instalados, miniaturas (`commands.rs`, `utils.rs`) | `NSWorkspace.runningApplications`, `NSRunningApplication.activate`, `CGWindowListCopyWindowInfo` + Acessibilidade (`AXUIElement`) para janelas por app, `NSWorkspace.icon(forFile:)`, varredura de `/Applications`, `~/Applications`, `/System/Applications`; miniaturas com **ScreenCaptureKit**. "Abrir nova instância" = `open -n`. |
| Substituir a barra/Dock (`sync_appbar`, `toggle_dock`, `change_dock_mode`) | Decisão 2 acima (`defaults` da Dock + `killall Dock`), com restauração. |
| Mídia (`media_*`, `open_media_source_app`) | MediaRemote é bloqueado para apps de terceiros desde macOS 15.4; usar a técnica do *mediaremote-adapter* (framework carregado via `/usr/bin/perl`, que tem o entitlement) ou, se falhar, AppleScript para Spotify e Música. Escolha final é do construtor após um teste rápido. |
| Visualizador de áudio (WASAPI loopback em `services.rs`) | Core Audio **process tap** global (macOS 14.4+) → FFT com `rustfft` (já é dependência) → mesmas 5 bandas. |
| Volume, mute, mixer por app (`set_volume`, `get_audio_sessions`, `set_app_volume`) | Volume/mute do sistema: Core Audio (`kAudioDevicePropertyVolumeScalar`). Volume **por app**: não existe no macOS sem driver — ver Diferenças. |
| Brilho | `DisplayServices` (framework privado, `DisplayServicesGetBrightness/SetBrightness`). |
| Wi-Fi (`wifi.rs`, `get_wifi_status`, `set_wifi_state`) | **CoreWLAN** (`CWWiFiClient`): estado, ligar/desligar, listar, escanear, conectar, desconectar. Esquecer rede: `networksetup -removepreferredwirelessnetwork` (pode pedir admin). |
| Bluetooth (`bluetooth.rs`, `get/set_bluetooth_state`) | **IOBluetooth**: listar, conectar/desconectar, parear, desparear; ligar/desligar via `IOBluetoothPreferenceSetControllerPowerState`. Descoberta: `IOBluetoothDeviceInquiry`. |
| Bateria dos fones (`buds.rs`) | AirPods/Beats: atributos de bateria do `IOBluetoothDevice` (ou anúncio BLE de proximidade via CoreBluetooth como hoje). Fast Pair e Galaxy Buds: RFCOMM via IOBluetooth, mesma lógica de parse. |
| Bateria, modo economia (`battery.rs`) | IOKit `IOPSCopyPowerSourcesInfo` + `AppleSmartBattery` (saúde, ciclos). Economia = **Modo de Pouca Energia** (`NSProcessInfo.isLowPowerModeEnabled`). |
| Fone no P2 (`jack.rs`) | Core Audio: `kAudioDevicePropertyDataSource` do alto-falante embutido (fones vs. alto-falantes) com listener. |
| USB (`usb.rs`) | IOKit `IOServiceAddMatchingNotification` (USB, com agrupamento por dispositivo); discos via DiskArbitration. |
| Discos e "extras" da Dock (`dock_extras.rs`) | `NSWorkspace` volumes montados, ejetar com `NSWorkspace.unmountAndEjectDevice`. |
| CPU, RAM, disco, rede | `host_processor_info`, `host_statistics64`, `statfs`, `getifaddrs` com contadores. |
| Cor de destaque | `NSColor.controlAccentColor`. |
| Prateleira (drop de arquivos/texto, `shelf.rs`, `shelf_preview.rs`) | Drop pelo próprio Tauri (`dragDropEnabled`) — no Mac funciona sem os contornos OLE. Miniaturas: QuickLook (`QLThumbnailGenerator`); prévia de Office continua via `zip`. |
| Bandeja/menu (`tray.rs`) | Ícone do Bloom na barra de menus com o mesmo menu. "Ícones da bandeja" de outros apps (`get_tray_apps`, `activate_tray_icon`) — ver Diferenças. |
| Atalhos globais (teclas antes do Windows) | `CGEventTap` (precisa Acessibilidade). |
| Abrir telas de ajustes (`open_wifi_settings` etc.) | URLs `x-apple.systempreferences:` correspondentes. |
| Autostart, clipboard, dialog, opener, drag | Plugins Tauri já suportam Mac. |
| Alertas de agentes (`agent_alerts.rs`, `integrations/agent-hook.ps1`) | Ver decisão 5: gancho `sh` novo, inspeção de processos com `libproc`, transporte Unix no lugar do que for Windows. |
| Atualizador (`updater.rs`) | Desligado no Mac (comandos respondem "não suportado"). |

## Diferenças assumidas (calls do construtor, a interface deve se adaptar)

- **Volume por app**: macOS não tem. `get_audio_sessions` devolve lista vazia; o mixer some da UI no Mac.
- **Ícones de bandeja de outros apps**: o Mac não expõe os ícones da barra de menus dos outros apps. `get_tray_apps` devolve vazio; o botão some no Mac.
- **Esconder o OSD nativo de volume/brilho** (`hide_native_osd`): não é possível de forma limpa; no-op.
- **Task View / Win+1..9 / botão Iniciar**: mapear para Mission Control, ⌥+1..9 (configurável) e Launchpad/Spotlight respectivamente; textos ajustados.
- **Modo avião**: não existe no Mac; botão some.
- Comandos sem equivalente retornam erro explícito `"unsupported on macOS"` em vez de fingir sucesso, e a interface esconde o controle correspondente via um `platform` exposto pelo backend.

## Estrutura do código

- Cada arquivo de área ganha divisão interna `#[cfg(windows)] mod win;` / `#[cfg(target_os = "macos")] mod mac;` (ou arquivos irmãos `wifi/win.rs`, `wifi/mac.rs`), com as funções `#[tauri::command]` finas chamando a implementação da plataforma. `commands.rs` e `services.rs` (4,7 mil linhas cada) são quebrados por área no processo, já que são eles que misturam tudo.
- `Cargo.toml`: dependências Windows (`windows`, `windows-core`, `windows-collections`, `wmi`, `webview2-com`) vão para `[target.'cfg(windows)'.dependencies]`; Mac ganha `[target.'cfg(target_os = "macos")'.dependencies]` com crates `objc2-*` (AppKit, Foundation, CoreWLAN, IOBluetooth, CoreAudio, ScreenCaptureKit, QuickLookThumbnailing), `core-foundation`, `io-kit-sys`.
- Interface: um módulo `platform.ts` lê a plataforma uma vez; componentes escondem controles sem suporte e a Ilha usa a geometria do notch.
- `tauri.conf.json`: `beforeDevCommand`/`beforeBuildCommand` funcionam com npm também (ou instalar bun). Bundle `.app` para Mac.

## Ordem sugerida

1. Toolchain + build mínimo: Windows atrás de `cfg`, comandos Mac como stubs, app abre no Mac com Ilha e Dock visíveis.
2. Janelas e cliques (nível, Spaces, região de clique), notch.
3. Dock (apps, ícones, focar, abrir, substituir Dock da Apple com restauração).
4. Mídia + visualizador.
5. Central de controles (volume, brilho, Wi-Fi, Bluetooth) e status (bateria, CPU/RAM/rede).
6. Aparelhos (fones, P2, USB, discos), prateleira, alertas, onboarding com permissões.

Cada etapa termina com o app abrindo e a parte nova verificada à mão no Mac.

## Deixado para o construtor

- Qual crate exata para cada API e a técnica final de "tocando agora".
- Divisão fina de `commands.rs`/`services.rs`.
- Detalhes visuais da Ilha nascendo do notch (raios, animação de mola), seguindo o estilo atual.
