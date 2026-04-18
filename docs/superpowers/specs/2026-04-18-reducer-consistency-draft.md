# Reducer Consistency — Draft Spec

**Date:** 2026-04-18
**Status:** Draft — deferred (P2 architectural nits from PR #4 review)

---

## Motivation

Three spots in `src/input.rs` mutate `AppState` directly through the `Arc<Mutex<AppState>>`
lock, bypassing the `handle_action` reducer. Every other state transition in the codebase
flows through `Action → handle_action`; these exceptions are internally consistent but
diverge from the established pattern. Converging them would:

- Keep the reducer as the single source of truth for state transitions.
- Make the state machine fully reconstructible from an `Action` log (useful for tests
  and, later, for replayable debugging).
- Remove the need for `input.rs` to carry `Arc<Mutex<AppState>>` into helper functions
  that are otherwise pure.

---

## Sites to Fix

### 1. `handle_create_swap_key` — inline completion clear

`src/input.rs` around the `completions_showing` branch:

```rust
// Any other key: clear completions inline, then forward to normal handler
{
    let mut s = state.lock().expect("state mutex poisoned");
    if let Some(m) = s.create_swap_modal.as_mut() {
        m.completions.clear();
        m.completion_sel = None;
    }
}
handle_form_key(/* … */)
```

`Action::CreateSwapClearCompletions` already exists and does the same thing. The issue:
the key that triggered the clear still needs to be forwarded to `handle_form_key`,
which currently depends on the completions being cleared *before* the forward so the
next frame sees a consistent state.

**Proposed fix:** make `resolve_key` able to return a *sequence* of actions (or a
composite "clear-then-forward" variant). Two shapes to consider:

- **A.** Change `resolve_key`'s return type to `Vec<Action>` (or `SmallVec<[Action; 2]>`)
  and dispatch them in order in `main.rs`. This is a broader refactor touching every
  call site.
- **B.** Introduce a single `Action::CreateSwapClearAndForward(crossterm::event::Event)`
  variant that the reducer handles by clearing completions and then running the same
  logic `CreateSwapInputEvent` runs. Narrower, but adds a new variant that only exists
  for this one case.

Recommendation: **A** — a `Vec<Action>` return type is a one-time change and unlocks
any future "one key, multiple effects" case without per-case variants.

### 2. `validate_and_submit` — validation error writes

Same file, `validate_and_submit`:

```rust
let err = |msg: &str| -> Option<Action> {
    let mut s = state.lock().expect("state mutex poisoned");
    if let Some(m) = s.create_swap_modal.as_mut() {
        m.validation_error = Some(msg.to_string());
    }
    None
};
```

**Proposed fix:** add `Action::CreateSwapSetValidationError(Option<String>)` and make
`validate_and_submit` a pure function that returns `Result<Action, Action>` (Ok =
submit, Err = set-validation-error). If (1) lands with `Vec<Action>`, this becomes
trivial.

### 3. `handle_create_swap_key` — reads via lock

The function locks `state` to read modal field values into local variables, then drops
the lock before handing them to `handle_form_key`. This read-only snapshot is fine
(not a bypass), but it's ~30 lines of boilerplate that would disappear if the reducer
owned the "form → action" translation. Low priority; included here for completeness.

---

## Out of Scope

- The background-task pattern (`spawn_blocking` in `main.rs`) already sends actions
  via `action_tx` and is not touched by this refactor.
- The mutex is not removed — render still reads `&AppState` under lock. Only the
  *mutation* paths converge on `handle_action`.

---

## Testing

- After the refactor, `resolve_key` takes `&AppState` (no `Arc<Mutex>`), making it
  straightforwardly unit-testable without constructing a mutex.
- Add a test covering the clear-and-forward sequence: pressing a character while
  completions are showing should produce both `CreateSwapClearCompletions` and
  `CreateSwapInputEvent(_)` in order.
- `validate_and_submit` becomes testable as a pure function returning an action.

---

## Risks / Notes

- Changing `resolve_key`'s return type touches `main.rs` and all tests in `input.rs`.
  Estimated ~30 test updates (`rk(...)` helper would need to flatten to a single
  action or return `Vec`).
- No behavior change is intended — this is a pure refactor to a single pattern.
