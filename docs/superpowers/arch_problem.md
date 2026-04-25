  PROBLEMAS

  P1: Arc<Mutex<AppState>> vazando para input.rs (GRAVE)

  Arquivo: input.rs:17 — KeyContext carrega &Arc<Mutex<AppState>>

  resolve_key deveria ser função pura: (KeyEvent, StateSnapshot) → Option<Action>. Em vez disso, faz múltiplos locks dentro do resolver:

  - input.rs:42-44 — lock para checar create_swap_modal
  - input.rs:99-100 — lock dentro de handle_devices_key para confirm_off_delete
  - input.rs:106-109 — outro lock para ler fields do modal
  - input.rs:129-134 — mais um lock para pegar device path
  - input.rs:151-158 — lock para checar is_active

  Pior: input.rs:272-276 faz mutação direta dentro do resolver:
  let mut s = state.lock().expect("state mutex poisoned");
  if let Some(m) = s.create_swap_modal.as_mut() {
      m.completions.clear();
      m.completion_sel = None;
  }

  Isso quebra o pattern Action/Reducer. O resolver deveria retornar ações, não mutar estado.

  Mesma violação em validate_and_submit (input.rs:416-420) — seta validation_error diretamente no state.

  Recomendação: Extrair todo estado necessário para KeyContext como valores (não referência ao Mutex). Criar actions para ClearCompletions e SetValidationError.

  ---
  P2: main.rs quebra abstração de plataforma (MODERADO)

  Arquivo: main.rs:27-28:
  use platform::linux::LinuxBackend;
  use platform::linux::create_swap::run_create_swap_steps;

  E em main.rs:139:
  let backend = LinuxBackend::new();

  O main.rs instancia LinuxBackend diretamente dentro de spawn_blocking para device ops, bypassing a factory. Se portar para macOS, esse código não compila.

  Recomendação: As operações de device (swap_on/swap_off) devem usar o trait. Passar um Arc<dyn SwapBackend> ou mover lógica de device ops para Collector.

  ---
  P3: Collector::collect bloqueia executor tokio (MODERADO)

  Arquivo: collector.rs:21 — pub async fn collect(&mut self) -> Result<MemSnapshot>

  Marcada como async mas faz zero async work. Todas chamadas internas são síncronas:
  - self.backend.system_ram()? — lê sysinfo
  - self.backend.swap_devices()? — lê /proc/swaps + probe block devices
  - self.backend.process_list()? — itera todos PIDs em /proc, lê status+stat por PID

  Isso bloqueia o executor tokio. Com muitos processos (centenas), process_list pode levar dezenas de ms. Em main.rs:96, essa call é feita no branch tick.tick() do
  tokio::select!, o que congela render e input por esse tempo.

  Recomendação: Wrap com tokio::task::spawn_blocking ou move Collector para thread dedicada.

  ---
  P4: AppState god object — 20+ campos, 1165 linhas (ESCALABILIDADE)

  Arquivo: app.rs

  Todos campos de todas tabs (Overview, Processes, Devices, CreateSwap, ConfirmOffDelete) vivem numa struct flat:

  pub struct AppState {
      pub active_tab: Tab,
      pub ram_history: VecDeque<(Instant, u64)>,    // Overview
      pub processes: Vec<ProcessRow>,                 // Processes
      pub selected_dev: usize,                        // Devices
      pub create_swap_modal: Option<CreateSwapModal>, // CreateSwap wizard
      pub confirm_off_delete: Option<ConfirmOffDelete>,// Delete dialog
      // ... 15+ more fields
  }

  handle_action tem 30+ match arms. Adicionar feature = mais campos + mais arms.

  Recomendação: Extrair sub-estados: ProcessState, DeviceState, CreateSwapState. AppState contém sub-estados + campos globais (tab, snapshot, error). Match arms delegam para
  sub-handlers.

  ---
  P5: Action enum com 30+ variants (ESCALABILIDADE)

  Arquivo: actions.rs — 10 variants só para CreateSwap modal.

  Recomendação: Agrupar em sub-enums:
  enum Action {
      Global(GlobalAction),
      Process(ProcessAction),
      Device(DeviceAction),
      CreateSwap(CreateSwapAction),
  }

  ---
  P6: Cloning desnecessário em UpdateSnapshot (PERFORMANCE)

  Arquivo: app.rs:158-171:
  self.devices = snapshot.devices.clone();
  self.processes = snapshot.processes.clone();
  // ... later:
  self.current = Some(snapshot);

  Devices e processes são clonados para os campos, e snapshot original (que contém os mesmos vecs) é armazenado em current. Double ownership.

  Recomendação: Desestruturar snapshot:
  let MemSnapshot { devices, processes, .. } = snapshot;
  self.devices = devices;
  self.processes = processes;
  Ou eliminar current e derivar campos dele.

  ---
  P7: linux módulo compilado incondicionalmente (BUILD)

  Arquivo: platform/mod.rs:4 — pub mod linux; sem #[cfg(target_os = "linux")]

  Todos outros módulos têm cfg gate. linux não. Build vai quebrar em macOS/Windows porque linux/mod.rs usa /proc, nix::libc::swapon, ioctl, etc.

  ---
  P8: compute_path_completions faz sync I/O no tokio thread (MENOR)

  Arquivo: input.rs:456-491 — std::fs::read_dir síncrono.

  TODO no código reconhece isso. Network mounts (NFS) podem travar event loop.

  ---
  P9: Background tasks sem cancelamento (MENOR)

  Arquivo: main.rs:167-199 — spawn_blocking para create-swap

  Se usuário aperta Esc durante progresso, modal fecha mas task continua. Arquivo parcial pode ficar em disco. TODO reconhecido em input.rs:312-316.

  ---
  P10: std::sync::Mutex com .expect("poisoned") em ~20 sites

  Toda operação de lock usa expect("state mutex poisoned"). Se qualquer thread panic, aplicação inteira crasha. Em TUI isso pode deixar terminal em estado sujo.

  Alternativa: parking_lot::Mutex (sem poisoning, sem unwrap) ou tratar poison recovery.

  ---
  Resumo por Prioridade

  ┌────────────┬───────────────────────────────────────────────┬────────────────────────────────┐
  │ Prioridade │                     Issue                     │            Impacto             │
  ├────────────┼───────────────────────────────────────────────┼────────────────────────────────┤
  │ ALTA       │ P1 — Mutex em input resolver + mutação direta │ Viola pattern, risco deadlock  │
  ├────────────┼───────────────────────────────────────────────┼────────────────────────────────┤
  │ ALTA       │ P2 — LinuxBackend hardcoded em main           │ Quebra portabilidade           │
  ├────────────┼───────────────────────────────────────────────┼────────────────────────────────┤
  │ MÉDIA      │ P3 — Collector bloqueia executor              │ UI freeze com muitos processos │
  ├────────────┼───────────────────────────────────────────────┼────────────────────────────────┤
  │ MÉDIA      │ P4/P5 — God object + enum gigante             │ Escalabilidade de features     │
  ├────────────┼───────────────────────────────────────────────┼────────────────────────────────┤
  │ MÉDIA      │ P7 — linux mod sem cfg                        │ Build quebra cross-platform    │
  ├────────────┼───────────────────────────────────────────────┼────────────────────────────────┤
  │ BAIXA      │ P6 — Cloning desnecessário                    │ Performance marginal           │
  ├────────────┼───────────────────────────────────────────────┼────────────────────────────────┤
  │ BAIXA      │ P8/P9 — I/O sync, sem cancelamento            │ Edge cases                     │
  ├────────────┼───────────────────────────────────────────────┼────────────────────────────────┤
  │ BAIXA      │ P10 — Mutex poisoning                         │ Crash recovery                 │
  └────────────┴───────────────────────────────────────────────┴────────────────────────────────┘

  Architecture is solid for current scope. Main risks: P1 (purity violation) and P2 (platform leak) should be fixed before adding more features. P3/P4/P5 become pressing as
  app grows.
