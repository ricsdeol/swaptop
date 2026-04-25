# Design: Refatorar `src/app.rs` em módulos por domínio

**Data:** 2026-04-24  
**Status:** Aprovado  
**Motivação:** `src/app.rs` atingiu ~1500 linhas com `handle_action` monolítico. Difícil navegar, testar e manter.

---

## 1. Objetivo

Quebrar `src/app.rs` em módulos Rust separados, **espelhando a estrutura de `src/ui/`**, mantendo:
- Coesão de domínio (cada arquivo = uma responsabilidade)
- Acesso direto aos campos de `AppState` (zero encapsulamento degradado)
- Testes colocados junto ao código que testam
- Compilação limpa e zero warnings

---

## 2. Estrutura Final

```
src/app/
  mod.rs           # AppState struct + handle_action dispatcher + helpers cross-cutting
  snapshot.rs      # UpdateSnapshot orquestrador + history helpers + tests
  devices.rs       # DeviceUp, DeviceDown, ExecuteDeviceOp, DeviceOpUpdate, ConfirmOffDelete, RequestConfirm, CancelConfirm + tests
  processes.rs     # NavigateUp, NavigateDown, SortBy, Filter* + tests
  create_swap.rs   # Open*, Close*, Focus*, Submit, Progress, Completion* + tests
```

---

## 3. Mapeamento Ações → Módulos

| Ação | Módulo | Handler |
|------|--------|---------|
| `Quit`, `NextTab`, `PrevTab`, `SelectTab`, `CollectStarted`, `CollectFinished`, `SetError` | `mod.rs` | Inline trivial |
| `UpdateSnapshot` | `snapshot.rs` | `apply_snapshot()` |
| `DeviceUp`, `DeviceDown`, `RequestConfirm`, `CancelConfirm`, `ExecuteDeviceOp`, `DeviceOpUpdate` | `devices.rs` | Métodos privados |
| `RequestConfirmOffDelete`, `ToggleConfirmDeleteFile`, `CancelConfirmOffDelete` | `devices.rs` | Métodos privados |
| `NavigateUp`, `NavigateDown`, `SortBy`, `EnterFilterMode`, `FilterChar`, `FilterBackspace`, `ExitFilterMode` | `processes.rs` | Métodos privados |
| `OpenCreateSwap`…`CreateSwapClearCompletions` | `create_swap.rs` | Métodos privados |

---

## 4. Solução do Orquestrador `UpdateSnapshot`

**Problema:** `UpdateSnapshot` toca history + devices + processes + selection clamping + error cleanup — cross-cutting.

**Solução:** `snapshot.rs` contém o orquestrador `apply_snapshot()`, mas delega reações de domínio para callbacks nos módulos:

```rust
// app/snapshot.rs
fn apply_snapshot(&mut self, snapshot: MemSnapshot) {
    // 1. History (cross-cutting, fica aqui)
    self.push_history(&snapshot);
    
    // 2. Domain data
    self.devices = snapshot.devices.clone();
    self.processes = snapshot.processes.clone();
    
    // 3. Domain-specific reactions (delegado)
    self.on_processes_updated();  // em processes.rs
    self.on_devices_updated();    // em devices.rs
    
    // 4. Snapshot metadata
    self.current = Some(snapshot);
    self.last_collect_completed = Instant::now();
    
    // 5. Clear stale errors
    self.clear_stale_errors();
}
```

```rust
// app/processes.rs
pub(crate) fn on_processes_updated(&mut self) {
    self.sort_processes();
    let len = self.filtered_len();
    self.selected_row = if len > 0 { self.selected_row.min(len - 1) } else { 0 };
}
```

```rust
// app/devices.rs
pub(crate) fn on_devices_updated(&mut self) {
    if !self.devices.is_empty() {
        self.selected_dev = self.selected_dev.min(self.devices.len() - 1);
    }
}
```

**Princípio:** O orquestrador coordena a ordem. Cada domínio reage às mudanças em seu próprio arquivo. Se amanhã precisar de lógica adicional quando devices mudam, vai em `devices.rs`.

---

## 5. Visibilidade

- `AppState` campos: permanecem como estão (`pub` ou implícito no struct)
- Métodos de callback (`on_processes_updated`, `on_devices_updated`): `pub(crate)`
- Handlers de ação: privados (`fn` sem qualifier)
- Helpers reutilizáveis (`sort_processes`, `filtered_len`): `pub(crate)` se usados cross-module

Nenhum campo precisa ficar mais exposto do que já está.

---

## 6. Testes

Cada módulo `.rs` contém seu próprio `#[cfg(test)] mod tests`.

| Módulo | Tests movidos |
|--------|---------------|
| `snapshot.rs` | `update_snapshot_appends_to_history`, `history_is_capped`, `update_snapshot_clears_error`, `update_snapshot_stores_devices`, `history_values_match`, `update_snapshot_sorts`, `update_snapshot_clamps_selected_row/dev` |
| `devices.rs` | `device_up/down`, `request/cancel_confirm`, `execute_device_op`, `device_op_update`, `confirm_off_delete` |
| `processes.rs` | `navigate_up/down`, `sort_by`, `filter_char/backspace`, `enter/exit_filter`, `filtered_len` |
| `create_swap.rs` | Todos os tests de Phase 5 (open, close, focus, toggle, submit, progress, completions, validation) |
| `mod.rs` | Tests triviais de `Quit`, `NextTab`, `PrevTab`, `SelectTab`, `is_root` |

Helpers de test (`make_caps`, `make_snapshot`, `make_device`, `make_process`): ficam em `mod.rs` como `pub(crate)` para reutilização entre módulos.

---

## 7. Tamanho Estimado Pós-Refatoração

| Arquivo | Linhas estimadas | Conteúdo |
|---------|------------------|----------|
| `mod.rs` | ~200 | Struct, `new()`, `handle_action` dispatcher, helpers triviais, test helpers |
| `snapshot.rs` | ~150 | `apply_snapshot`, history helpers, tests |
| `processes.rs` | ~300 | Handlers + `on_processes_updated` + sort/filter + tests |
| `devices.rs` | ~280 | Handlers + confirm actions + `on_devices_updated` + tests |
| `create_swap.rs` | ~450 | Handlers complexos de modal + tests |

**Máximo por arquivo: ~450 linhas** (vs. 1500 hoje).

---

## 8. Passos de Implementação

1. Criar diretório `src/app/` e `mod.rs` com o struct + dispatcher
2. Mover `UpdateSnapshot` → `snapshot.rs` com callbacks
3. Mover handlers de devices → `devices.rs`
4. Mover handlers de processes → `processes.rs`
5. Mover handlers de create_swap → `create_swap.rs`
6. Confirm actions (`RequestConfirm`, `CancelConfirm`) já estão incluídos em `devices.rs` (são confirmações de operações de device)
7. Mover tests respectivos para cada módulo
8. Atualizar `src/lib.rs`/`main.rs` para importar `app::AppState`
9. Rodar `cargo test`, `cargo clippy`, `cargo fmt`

---

## 9. Riscos e Mitigação

| Risco | Mitigação |
|-------|-----------|
| Diff muito grande no git | Fazer em um único commit bem descrito; é refatoração mecânica |
| Tests quebram por imports | Compilar após cada módulo movido |
| `pub(crate)` excessivo | Revisar no final; idealmente só callbacks e test helpers |
| `filtered_len` usado em 2 módulos | Deixa em `mod.rs` como `pub(crate)` ou duplica se trivial |

---

## 10. Critérios de Sucesso

- [ ] `cargo build` zero warnings
- [ ] `cargo clippy -- -D warnings` passa
- [ ] `cargo fmt --check` passa
- [ ] `cargo test` todos passam
- [ ] Nenhuma mudança de comportamento (refatoração pura)
- [ ] `src/app.rs` deixa de existir (substituído por `src/app/mod.rs`)
