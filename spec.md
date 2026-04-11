# swaptop — TUI para gerenciamento de swap

> Documento de spec para brainstorming inicial no Claude Code.
> Linguagem: Rust. Framework TUI: Ratatui + crossterm. Runtime: tokio.
> Plataformas alvo: Linux (fase 1–5), macOS (fase 2 em diante), BSD/Windows (futuro).

---

## Visão geral

`swaptop` é um TUI interativo para terminal que permite monitorar e gerenciar
memória swap em tempo real. Inspirado no `htop` e `btop`, mas focado
exclusivamente em swap e memória, com capacidade de gerenciamento ativo
(ligar/desligar dispositivos de swap, criar swap files).

O app deve rodar como binário standalone (sem runtime externo), ser
cross-platform via abstração de backend, e manter histórico de métricas
desde a inicialização.

---

## Decisões técnicas já tomadas

### Stack principal

| Camada         | Crate              | Justificativa                                      |
|----------------|--------------------|----------------------------------------------------|
| TUI            | `ratatui`          | Widgets nativos (Chart, Table, Gauge), immediate-mode, sub-ms render |
| Terminal       | `crossterm`        | Cross-platform, feature `event-stream` para async  |
| Async runtime  | `tokio`            | `tokio::select!` multiplexando tick/render/input   |
| Sistema        | `sysinfo`          | Cross-platform: RAM, Swap total, lista de processos |
| Syscalls Linux | `nix`              | `swapon()`, `swapoff()`, `kill()` via libc         |
| Errors         | `color-eyre`       | Error handling ergonômico                          |
| Formatação     | `human_bytes`      | "2.3 GB", "512 MB"                                 |

### Cargo.toml esperado

```toml
[dependencies]
ratatui     = "0.29"
crossterm   = { version = "0.28", features = ["event-stream"] }
tokio       = { version = "1", features = ["full"] }
tokio-util  = "0.7"
futures     = "0.3"
sysinfo     = "0.32"
nix         = { version = "0.29", features = ["process", "signal", "mount"] }
color-eyre  = "0.6"
human_bytes = "0.4"
glob        = "0.3"   # macOS swapfile discovery
```

---

## Arquitetura de abstração de plataforma

### Problema central

Cada OS gerencia swap de forma diferente:

- **Linux**: syscalls `swapon`/`swapoff`, `/proc/swaps`, `/proc/PID/smaps`
  para swap por processo.
- **macOS**: daemon `dynamic_pager`, arquivos em `/private/var/vm/swapfile*`,
  controlado via `launchctl`, sem API pública para swap por processo.
- **Windows**: `pagefile.sys`, WMI `Win32_PageFileSetting`, sem equivalente
  a swapon/swapoff.
- **FreeBSD/OpenBSD**: `kvm_getswapinfo`, syscalls compatíveis com Linux.

### Trait SwapBackend — contrato público

```rust
// src/platform/mod.rs

pub trait SwapBackend: Send + Sync {
    /// Totais globais de swap (total, usado, livre, %)
    fn system_swap(&mut self) -> Result<SwapInfo>;

    /// RAM global (total, usado, livre, %)
    fn system_ram(&mut self) -> Result<SwapInfo>;

    /// Lista de dispositivos/arquivos de swap ativos no sistema
    fn swap_devices(&mut self) -> Result<Vec<SwapDevice>>;

    /// Swap consumido por processo específico em bytes.
    /// Retorna 0 se o OS não suportar (ver Capabilities.has_per_process).
    fn process_swap(&self, pid: u32) -> u64;

    /// Ativa um device ou arquivo de swap
    fn swap_on(&self, device: &std::path::Path) -> Result<()>;

    /// Desativa um device ou arquivo de swap
    fn swap_off(&self, device: &std::path::Path) -> Result<()>;

    /// Reset: swap_off + swap_on no mesmo device
    fn swap_reset(&self, device: &std::path::Path) -> Result<()> {
        self.swap_off(device)?;
        self.swap_on(device)
    }

    /// O que este backend consegue fazer neste OS/configuração
    fn capabilities(&self) -> Capabilities;
}
```

### Tipos compartilhados

```rust
// src/platform/types.rs

pub struct SwapInfo {
    pub total:   u64,   // bytes
    pub used:    u64,   // bytes
    pub free:    u64,   // bytes
    pub percent: f32,   // 0.0–100.0
}

pub struct SwapDevice {
    pub path:     PathBuf,
    pub total:    u64,
    pub used:     u64,
    pub priority: i16,
    pub kind:     SwapKind,
    pub active:   bool,
}

pub enum SwapKind {
    Partition,      // /dev/sdaX
    File,           // /swapfile, /var/swap
    Zram,           // /dev/zram0 (Linux)
    DynamicPager,   // /private/var/vm/swapfile* (macOS)
}

pub struct ProcessRow {
    pub pid:       u32,
    pub name:      String,
    pub user:      String,
    pub rss:       u64,   // RAM usada (bytes)
    pub vms:       u64,   // memória virtual (bytes)
    pub swap:      u64,   // swap usado (bytes) — 0 se OS não suportar
    pub cpu_pct:   f32,   // % CPU
}

pub struct Capabilities {
    pub can_swap_on:      bool,  // Linux: true | macOS: false (dynamic_pager)
    pub can_swap_off:     bool,  // Linux: true | macOS: false
    pub has_per_process:  bool,  // Linux: true | macOS: false
    pub has_device_list:  bool,  // todos: true
    pub can_create_swap:  bool,  // Linux: true | macOS: false
    pub requires_root:    bool,  // todos: true para operações de controle
}

/// Snapshot completo coletado a cada tick
pub struct MemSnapshot {
    pub timestamp:  std::time::Instant,
    pub ram:        SwapInfo,
    pub swap:       SwapInfo,
    pub devices:    Vec<SwapDevice>,
    pub processes:  Vec<ProcessRow>,
}
```

### Implementações por OS

```
src/platform/
├── mod.rs          — trait SwapBackend + re-exports
├── types.rs        — todos os structs compartilhados
├── factory.rs      — fn detect() -> Box<dyn SwapBackend>
├── linux.rs        — LinuxBackend
├── macos.rs        — MacosBackend
├── windows.rs      — WindowsBackend (stub)
└── bsd.rs          — BsdBackend (futuro)
```

**factory.rs — detecção automática:**
```rust
pub fn detect() -> Box<dyn SwapBackend> {
    #[cfg(target_os = "linux")]
    return Box::new(linux::LinuxBackend::new());
    #[cfg(target_os = "macos")]
    return Box::new(macos::MacosBackend::new());
    #[cfg(target_os = "windows")]
    return Box::new(windows::WindowsBackend::new());
    #[cfg(any(target_os = "freebsd", target_os = "openbsd"))]
    return Box::new(bsd::BsdBackend::new());
}
```

**LinuxBackend — fontes de dados:**
- `sysinfo::System` → RAM e swap totais, lista de processos, CPU%
- `/proc/swaps` → dispositivos de swap ativos (path, tamanho, uso, prioridade)
- `/proc/PID/smaps` → campo `VmSwap:` para swap por processo (único jeito confiável)
- `nix::mount::swapon()` / `nix::mount::swapoff()` → controle

**MacosBackend — fontes de dados:**
- `sysinfo::System` → RAM e swap totais
- `glob("/private/var/vm/swapfile*")` + `fs::metadata` → lista e tamanho dos swap files
- `vm_stat` (parse de stdout via `Command`) → páginas paginadas/comprimidas
- `launchctl` → controle via subprocess (requer sudo + SIP desativado)
- `process_swap()` → retorna 0, `Capabilities.has_per_process = false`

**Regra de ouro para o collector:**
```rust
// collector.rs usa APENAS o trait — nunca importa linux.rs ou macos.rs diretamente
pub struct Collector {
    backend: Box<dyn SwapBackend>,
}
```

---

## Estrutura completa de arquivos

```
swaptop/
├── Cargo.toml
└── src/
    ├── main.rs             — tokio::main, inicializa tudo, passa backend detectado
    ├── tui.rs              — lifecycle terminal: enter/exit/suspend, CancellationToken
    ├── events.rs           — EventLoop async: tick_interval + frame_interval + crossterm
    ├── app.rs              — AppState + reducer de Actions (puro, sem I/O)
    ├── collector.rs        — coleta dados via SwapBackend, manda snapshots via mpsc
    ├── actions.rs          — enum Action { Navigate · Sort · Kill · SwapOff · SwapOn · Quit · ... }
    ├── platform/
    │   ├── mod.rs          — trait SwapBackend
    │   ├── types.rs        — structs compartilhados
    │   ├── factory.rs      — detect()
    │   ├── linux.rs        — LinuxBackend impl
    │   ├── macos.rs        — MacosBackend impl
    │   ├── windows.rs      — WindowsBackend stub
    │   └── bsd.rs          — BsdBackend futuro
    └── ui/
        ├── mod.rs          — render() principal, monta layout por fase/tab ativa
        ├── overview.rs     — Fase 1: gauges + charts de RAM e Swap
        ├── processes.rs    — Fase 2/3: tabela e detalhe de processo
        ├── devices.rs      — Fase 4: gerenciamento de dispositivos swap
        ├── create_swap.rs  — Fase 5: wizard criação de swap file
        └── statusbar.rs    — linha inferior com keybindings e alertas
```

---

## Loop de eventos assíncrono

```
tokio::select! em loop:
  ├── tick_interval (1s)   → Collector coleta MemSnapshot → mpsc → AppState atualiza
  ├── frame_interval (30fps) → Event::Render → terminal.draw() com &AppState
  └── EventStream crossterm → teclas → Action → AppState muta
```

Três tasks independentes:
1. **Main task** — event loop com `tokio::select!`
2. **Collector task** — `tokio::spawn`, coleta a cada tick e envia via `mpsc::unbounded`
3. **Render** — triggered por `Event::Render` no mesmo loop principal

AppState é acessado por `Arc<Mutex<AppState>>` entre collector e render.

---

## Estado da aplicação (app.rs)

```rust
pub struct AppState {
    // Navegação
    pub active_tab:    Tab,           // Overview | Processes | Devices | CreateSwap
    pub active_phase:  Phase,         // controla o que está disponível

    // Histórico (acumulado desde inicialização — nunca limpa)
    pub ram_history:   VecDeque<(Instant, u64)>,   // (timestamp, bytes_used)
    pub swap_history:  VecDeque<(Instant, u64)>,   // (timestamp, bytes_used)
    pub max_history:   usize,                       // padrão: 3600 pontos (1h a 1s/tick)

    // Dados atuais
    pub current:       Option<MemSnapshot>,

    // Fase 2/3 — processos
    pub processes:     Vec<ProcessRow>,
    pub sort_col:      SortColumn,    // Pid | Name | Rss | Swap | Cpu
    pub sort_dir:      SortDir,       // Asc | Desc
    pub selected_row:  usize,
    pub process_history: HashMap<u32, VecDeque<(Instant, u64)>>, // pid → histórico de swap

    // Fase 4 — devices
    pub devices:       Vec<SwapDevice>,
    pub selected_dev:  usize,

    // Fase 5 — criação
    pub create_form:   CreateSwapForm,

    // Meta
    pub capabilities:  Capabilities,
    pub error_msg:     Option<String>,   // banner de erro temporário
    pub uptime:        Duration,         // tempo de execução do app
}

pub enum Tab { Overview, Processes, Devices, CreateSwap }
pub enum SortColumn { Pid, Name, Rss, Swap, Cpu }
pub enum SortDir { Asc, Desc }
```

---

## Fase 1 — Overview: RAM e Swap global com histórico

**Escopo:**
- Gauge de RAM: `[████████░░] 7.2 GB / 16 GB (45%)`
- Gauge de Swap: `[███░░░░░░░] 1.1 GB / 4 GB (28%)`
- Chart RAM: sparkline/line chart com histórico desde início do app (eixo X = tempo)
- Chart Swap: idem
- Estatísticas textuais: total, usado, livre, % para RAM e Swap separados
- Totais de dispositivos de swap ativos (count + total size)
- Refresh automático a cada 1s (tick_interval)

**Layout sugerido:**
```
┌─────────────────────────────────────────────┐
│  RAM   [████████████░░░░░░] 7.2/16.0 GB 45% │
│  Swap  [████░░░░░░░░░░░░░░] 1.1/ 4.0 GB 28% │
├──────────────────┬──────────────────────────┤
│  RAM history     │  Swap history            │
│                  │                          │
│  ╭─╮             │          ╭──╮            │
│ ╭╯ ╰─           │     ╭───╯  ╰─           │
│─╯               │─────╯                   │
├──────────────────┴──────────────────────────┤
│  Devices: 2 ativos  |  Total: 6.0 GB        │
│  [q] sair  [Tab] mudar aba  [r] refresh      │
└─────────────────────────────────────────────┘
```

**Notas técnicas Fase 1:**
- Histórico usando `VecDeque<(Instant, u64)>` com max de 3600 pontos
- Chart do Ratatui usa `Vec<(f64, f64)>` — converter Instant para segundos desde início
- Gauge e Sparkline são widgets nativos do Ratatui — não precisa de lib externa
- No macOS: swap total vem de `sysinfo` (funciona), devices vêm de glob dos swapfiles
- Nenhuma permissão especial necessária para Fase 1 (só leitura)

---

## Fase 2 — Tabela de processos

**Escopo:**
- Tabela com colunas: PID | Nome | Usuário | RSS | Swap | CPU%
- Scroll com `j`/`k` ou setas
- Ordenação por qualquer coluna com `s` (toggle asc/desc)
- Coluna Swap fica cinza/vazia no macOS (Capabilities.has_per_process = false)
- Banner no topo explicando limitação de plataforma quando swap por processo não disponível
- Filtro básico por nome (tecla `/` abre input)

**Colunas e larguras sugeridas:**
```
PID     Nome              Usuário    RSS        Swap       CPU%
------  ----------------  ---------  ---------  ---------  ------
12345   firefox           ricardo    512.3 MB   128.0 MB   12.5%
 3821   code              ricardo    256.1 MB    64.0 MB    4.2%
   42   kswapd0           root         0.0  B     0.0  B    0.1%
```

**Notas técnicas Fase 2:**
- `sysinfo` fornece RSS, VMS, CPU% por processo
- Swap por processo: Linux via `/proc/PID/smaps` (campo `VmSwap:`), parsear cada processo
- Parsing de smaps é pesado — fazer em tokio task, não bloquear render
- Ordenação: `Vec::sort_by` no AppState, triggered por Action::SortBy
- Filtro: campo de texto em AppState, filtra `processes` antes de renderizar
- `process_history`: iniciar coleta desde Fase 1, mesmo que a tab não esteja aberta

---

## Fase 3 — Detalhe de processo com gráficos

**Escopo:**
- Tela de detalhe ao pressionar Enter na tabela de processos
- Exibe gráficos de histórico de swap e RAM do processo selecionado
- Histórico disponível desde o início da coleta (Fase 1 já armazena)
- Informações adicionais: caminho do executável, usuário, PID, threads, status
- Ação: `k` para matar o processo (com confirmação)
- Voltar com `Esc` ou `q`

**Layout sugerido:**
```
┌─ Processo: firefox (PID 12345) ───────────────┐
│ Usuário: ricardo  |  Threads: 48  |  Status: R │
│ Exec: /usr/lib/firefox/firefox                 │
├──────────────────┬─────────────────────────────┤
│  RAM (histórico) │  Swap (histórico)           │
│                  │                             │
│      ╭─╮         │   ─╮                       │
│  ╭───╯ ╰─        │    ╰────────               │
├──────────────────┴─────────────────────────────┤
│  Atual: RSS 512.3 MB  |  Swap 128.0 MB         │
│  [k] matar  [Esc] voltar                        │
└────────────────────────────────────────────────┘
```

**Notas técnicas Fase 3:**
- `process_history: HashMap<u32, VecDeque<(Instant, u64)>>` já preenchido desde Fase 1
- Gráfico de processo: mesmo Chart widget da Fase 1, só muda o dataset
- Kill: `nix::sys::signal::kill(Pid::from_raw(pid), Signal::SIGTERM)`
- Confirmação: modal simples com Paragraph + Block sobre o layout

---

## Fase 4 — Gerenciamento de dispositivos swap

**Escopo:**
- Lista de todos os dispositivos de swap (ativos e conhecidos/inativos)
- Colunas: Path | Tipo | Total | Usado | % | Prioridade | Status
- Ações disponíveis por device selecionado:
  - `o` — swap_on (ativar)
  - `f` — swap_off (desativar)
  - `r` — swap_reset (off + on, para liberar swap fragmentado)
- Confirmação obrigatória para todas as ações destrutivas
- Indicação visual clara de quais operações requerem root
- No macOS: botões desabilitados visualmente + mensagem `Controlado por dynamic_pager`
- No macOS: mostrar estado do daemon dynamic_pager (ativo/inativo via launchctl)

**Layout sugerido:**
```
┌─ Dispositivos de Swap ────────────────────────────────────────────────┐
│  Path                   Tipo       Total    Usado    %    Pri  Status  │
│ ▶ /dev/sda2             Partition  4.0 GB   1.1 GB  28%   -1  ATIVO   │
│   /swapfile             File       2.0 GB   0.0 GB   0%    0  INATIVO │
│   /dev/zram0            Zram       512 MB   200 MB  39%   100  ATIVO   │
├───────────────────────────────────────────────────────────────────────┤
│  [o] ativar  [f] desativar  [r] reset  [Enter] detalhe  [Esc] voltar  │
│  ⚠️  Operações requerem root                                           │
└───────────────────────────────────────────────────────────────────────┘
```

**Notas técnicas Fase 4:**
- Linux: `/proc/swaps` lista os ativos; devices inativos vêm de config (ou podem ser adicionados manualmente)
- `swapon`/`swapoff` requerem root — verificar `nix::unistd::geteuid() == 0` antes
- Se não for root: mostrar mensagem e sugerir `sudo swaptop` ou reexec com pkexec
- Reset: `swap_off` + pequeno delay (100ms) + `swap_on` — libera páginas fragmentadas
- macOS: mostrar estado via `Command::new("launchctl").arg("list")` + grep dynamic_pager

---

## Fase 5 — Criação de swap file

**Escopo:**
- Wizard passo-a-passo para criar novo swap file
- Inputs: caminho do arquivo, tamanho (com unidade: MB/GB), prioridade
- Validações: caminho válido, espaço em disco suficiente, arquivo não existente
- Execução dos passos em background task com progress bar
- Passos executados:
  1. `fallocate -l <size> <path>` (ou `dd` como fallback)
  2. `chmod 600 <path>`
  3. `mkswap <path>`
  4. `swapon <path>` (opcional, checkbox)
- Resumo final com sucesso/erro por passo
- No macOS: tela desabilitada com explicação (`dynamic_pager` gerencia automaticamente)

**Layout sugerido (formulário):**
```
┌─ Criar Swap File ─────────────────────────────┐
│                                               │
│  Caminho:   [/swapfile2              ]        │
│  Tamanho:   [2          ] [GB ▼]             │
│  Prioridade: [0          ]                    │
│  Ativar após criar: [x]                       │
│                                               │
│  Espaço disponível em /: 45.2 GB             │
│                                               │
│  [Criar]  [Cancelar]                          │
│                                               │
│  ─── Progresso ───────────────────────────── │
│  ✅ fallocate concluído                        │
│  ✅ chmod 600                                  │
│  ✅ mkswap                                     │
│  ⏳ swapon...                                  │
└───────────────────────────────────────────────┘
```

**Notas técnicas Fase 5:**
- Input de texto: `tui-textarea` crate ou widget simples com String + cursor pos no AppState
- Executar comandos via `tokio::process::Command` para não bloquear render
- Progress: `Vec<Step { name, status: Pending|Running|Done|Error }>` no AppState
- Validação de espaço: `sysinfo::Disks` ou `statvfs` via nix
- macOS: `Capabilities.can_create_swap = false` → tela mostra aviso e retorna

---

## Keybindings globais

| Tecla       | Ação                                    |
|-------------|------------------------------------------|
| `1`         | Tab Overview (Fase 1)                   |
| `2`         | Tab Processos (Fase 2/3)                |
| `3`         | Tab Devices (Fase 4)                    |
| `4`         | Tab Criar Swap (Fase 5)                 |
| `Tab`       | Próxima tab                             |
| `q` / `Q`   | Sair                                    |
| `r`         | Forçar refresh imediato                 |
| `?`         | Toggle ajuda                            |
| `j` / `↓`   | Mover seleção para baixo                |
| `k` / `↑`   | Mover seleção para cima                 |
| `s`         | Ordenar (na tabela de processos)        |
| `/`         | Filtrar (na tabela de processos)        |
| `Enter`     | Abrir detalhe do item selecionado       |
| `Esc`       | Voltar / fechar modal                   |

---

## Limitações por plataforma (para a UI comunicar ao usuário)

### macOS

| Funcionalidade          | Status          | Razão                                     |
|-------------------------|-----------------|-------------------------------------------|
| Swap total global       | ✅ disponível   | sysinfo lê via host_statistics64          |
| Lista de swapfiles      | ✅ disponível   | glob /private/var/vm/swapfile*            |
| Swap por processo       | ❌ indisponível | API privada do kernel, SIP protegido      |
| swap_on / swap_off      | ❌ indisponível | Gerenciado por dynamic_pager + launchctl  |
| Criar swap file         | ❌ indisponível | dynamic_pager gerencia automaticamente    |
| Reset swap              | ❌ indisponível | Sem controle programático                 |

A UI deve:
- Mostrar as funcionalidades disponíveis normalmente
- Desabilitar visualmente (cinza + ícone 🔒) o que não é suportado
- Mostrar tooltip/banner explicando a razão no macOS

---

## Perguntas em aberto para o brainstorming

1. **Nome do app**: `swaptop`, `swapui`, `swaptop`? Influencia o nome do binário.

2. **Persistência de config**: salvar preferências (sort padrão, tick rate, max_history)
   em `~/.config/swaptop/config.toml`? Usar `dirs` crate para path cross-platform.

3. **Histórico entre sessões**: por padrão o histórico é apenas in-memory.
   Faz sentido serializar para disco (SQLite via `rusqlite` ou arquivo binário)?

4. **Alertas**: threshold configurável de swap? Notificação no statusbar quando
   swap > X%? Bell character no terminal?

5. **Mouse support**: habilitar via crossterm `EnableMouseCapture`? Clique em linhas
   da tabela, scroll com mouse wheel?

6. **Cores e tema**: hardcoded ou configurável? Suporte a `NO_COLOR`?

7. **Modo não-interativo / output**: `-o json` para output de métricas (útil para scripts)?

8. **Reexec como root**: se detectar que não é root e precisar de operações privilegiadas,
   oferecer `sudo swaptop` ou `pkexec swaptop` automaticamente?

9. **Fase 3 — por processo no macOS**: investigar se `proc_pidinfo` via FFI expõe
   algo útil sem precisar de SIP desativado.

10. **zram no Linux**: merece tratamento especial? `zramctl` para detalhe de compressão?

---

## Ordem de implementação sugerida

```
Fase 1 → establece loop de eventos, collector, AppState, platform/linux.rs básico
Fase 2 → expande collector com smaps, adiciona ui/processes.rs e sorting
Fase 3 → adiciona process_history, detalhe de processo, kill action
Fase 4 → platform com swapon/swapoff, ui/devices.rs, root check
Fase 5 → wizard, tokio::process::Command, validações
```


TODO FOCO inicial deve ser para linux, onde todas as funcionalidades estão disponíveis. macOS e Windows podem ser alvos de fases futuras, com foco em leitura (monitoramento) antes de controle ativo.

Deixe somente o código extencivel para outros OS (Mac, Windows, BSD) — a implementação específica de cada um deve ser feita posteriormente, mas a arquitetura já deve contemplar isso desde o início.
