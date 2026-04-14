# Phase 6 — UX Improvements: Create Swap & Device Listing

Date: 2026-04-14

## Overview

Four improvements grouped into a single phase. They touch the create-swap
modal, the input subsystem, the device listing collector, and the confirm
modal for deactivation. Each is independent at the code level but ships
together for a cohesive UX update.

| Area | Summary |
|------|---------|
| 1. Cursor visibility | Real terminal cursor on focused text fields |
| 2. Path autocomplete | Tab-triggered popup listing fs entries |
| 3. Delete file on swapoff | Separate confirm modal for `SwapKind::File` |
| 4. Inactive swap discovery | Collector detects swap files at well-known paths |

---

## Area 1 — Cursor Visibility in Text Fields

### Problem

The create-swap form renders the `tui_input::Input` value as a padded
`[value                        ]` span, but never places the terminal cursor.
Users cannot tell where typed characters will appear or navigate within the
input.

### Design

In `ui/create_swap.rs::render_form`, after rendering a focused text field
(Path, Size, Priority):

1. Compute the visual column of the cursor:
   ```
   let visual_col = modal.{field}_input.value()
       [..modal.{field}_input.cursor()]
       .chars().count() as u16;
   ```
2. Compute absolute screen position:
   ```
   let cursor_x = field_rect.x + label_width + 1 /* "[" bracket */ + visual_col;
   let cursor_y = field_rect.y;
   ```
3. Call `f.set_cursor_position((cursor_x, cursor_y))`.

When the focused field is NOT a text field (SizeUnit, ActivateAfter, Submit),
`set_cursor_position` is not called, and Ratatui hides the terminal cursor
automatically on the next frame — no explicit hide logic needed.

### Files changed

- `src/ui/create_swap.rs` — `render_form` gains cursor placement logic.

### No new types, no new actions, no new dependencies.

---

## Area 2 — Path Tab-Autocomplete Popup

### Problem

Users must type full absolute paths manually. Typos are common; discoverability
of existing directories and files is poor.

### Design

#### New state in `CreateSwapModal`

```rust
pub completions: Vec<String>,
pub completion_sel: Option<usize>,
```

Both default to `Vec::new()` / `None` in `Default` impl.

#### Key flow

| Key | When | Action dispatched |
|-----|------|-------------------|
| `Tab` | Path focused, completions empty | Compute completions from fs, dispatch `CreateSwapSetCompletions(Vec<String>)` |
| `Tab` | Path focused, completions showing | Move selection down (wrap) |
| `Up` | Completions showing | `CreateSwapCompletionMove(-1)` |
| `Down` | Completions showing | `CreateSwapCompletionMove(1)` |
| `Enter` | Completions showing, item selected | `CreateSwapApplyCompletion` |
| `Esc` | Completions showing | `CreateSwapClearCompletions` |
| Any char | Completions showing | Clear completions first, then forward char to input |

#### Completion computation (inline in `input.rs`)

```rust
fn compute_path_completions(partial: &str) -> Vec<String> {
    let path = std::path::Path::new(partial);
    let (dir, prefix) = if partial.ends_with('/') {
        (path, "")
    } else {
        (path.parent().unwrap_or(Path::new("/")),
         path.file_name().and_then(|n| n.to_str()).unwrap_or(""))
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut results: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name().to_str()
                .map(|n| n.starts_with(prefix))
                .unwrap_or(false)
        })
        .map(|e| {
            let mut p = e.path().to_string_lossy().to_string();
            if e.path().is_dir() {
                p.push('/');
            }
            p
        })
        .collect();
    results.sort();
    results
}
```

This runs synchronously — `read_dir` on a single directory is < 1 ms.
No `spawn_blocking` needed. Returns `Vec<String>` owned by the caller.

#### Reducer (`app.rs`)

- `CreateSwapSetCompletions(Vec<String>)` — stores on modal, sets
  `completion_sel = if vec.is_empty() { None } else { Some(0) }`.
- `CreateSwapCompletionMove(i16)` — wraps `completion_sel` within bounds.
- `CreateSwapApplyCompletion` — reads `completions[sel]`, replaces
  `path_input` with `Input::from(value)`, clears `completions` and
  `completion_sel`.
- `CreateSwapClearCompletions` — clears `completions`, sets
  `completion_sel = None`.

#### Render (`ui/create_swap.rs`)

When `modal.completions` is non-empty, overlay a popup immediately below the
Path row:

- Width: same as the value span (30 chars + brackets).
- Height: `min(completions.len(), 6)` rows.
- Selected item: bg Cyan, fg Black.
- Uses `Clear` + `Block` with thin border.
- Z-order: rendered after the form block, so it overlaps lower fields.

### New actions

```rust
CreateSwapSetCompletions(Vec<String>),
CreateSwapCompletionMove(i16),
CreateSwapApplyCompletion,
CreateSwapClearCompletions,
```

### Files changed

- `src/actions.rs` — new variants.
- `src/create_swap.rs` — new fields on `CreateSwapModal`, updated `Default`.
- `src/app.rs` — reducer arms for new actions.
- `src/input.rs` — `compute_path_completions` function; Tab/Up/Down/Enter/Esc
  handling when completions are visible; clear completions on char input.
- `src/ui/create_swap.rs` — popup render logic.

### No new dependencies.

---

## Area 3 — Delete File on Swapoff (Separate Modal)

### Problem

When deactivating a swap file (`SwapKind::File`), users sometimes want to also
delete the underlying file. The current generic confirm modal offers no such
option.

### Design

#### New types

```rust
/// Stored in `AppState` when the "deactivate file" modal is open.
#[derive(Debug, Clone)]
pub struct ConfirmOffDelete {
    pub path: PathBuf,
    pub delete_file: bool,  // default: false
}
```

#### New `DeviceOpKind` variant

```rust
pub enum DeviceOpKind {
    On,
    Off,
    OffAndDelete, // swapoff + rm
    Reset,
}
```

#### Flow

1. User presses `f` on a device.
2. If `device.kind == SwapKind::File`:
   - Dispatch `Action::RequestConfirmOffDelete` instead of
     `Action::RequestConfirm(DeviceOpKind::Off)`.
   - Reducer sets `confirm_off_delete = Some(ConfirmOffDelete { path, delete_file: false })`.
3. If `device.kind != SwapKind::File`:
   - Existing flow unchanged (`confirm_action = Some(Off)`).

#### Modal render (`ui/devices.rs`)

Distinct from the generic confirm modal:

```
+============ Deactivate Swap File =============+
|                                               |
|  /var/swapfile                                |
|  This will deactivate the swap area.          |
|                                               |
|  [ ] also delete file (cannot be undone)      |
|                                               |
|  Space toggle  ·  s confirm  ·  Esc cancel    |
+===============================================+
```

- Border color: **Yellow** when `delete_file = false`,
  **Red** when `delete_file = true`.
- Checkbox styled with `Modifier::BOLD` when true.

#### Key handling

| Key | Action |
|-----|--------|
| `Space` | `ToggleConfirmDeleteFile` — flips `delete_file` |
| `s` / `Enter` | If `delete_file` → `ExecuteDeviceOp { kind: OffAndDelete }`, else `ExecuteDeviceOp { kind: Off }` |
| `Esc` | `CancelConfirmOffDelete` — sets `confirm_off_delete = None` |

#### Background task (`main.rs`)

The `ExecuteDeviceOp` dispatch in `main.rs` gains a new arm:

```rust
DeviceOpKind::OffAndDelete => {
    let off_result = backend.swap_off(&path);
    match off_result {
        Ok(()) => {
            // swapoff succeeded — attempt delete
            match std::fs::remove_file(&path) {
                Ok(()) => OpStatus::Done,
                Err(e) => OpStatus::Error(
                    format!("deactivated; delete failed: {e}")
                ),
            }
        }
        Err(e) => OpStatus::Error(e.to_string()),
    }
}
```

Error hierarchy:
- swapoff fails → `"swapoff: <msg>"` — file untouched.
- swapoff ok, rm fails → `"deactivated; delete failed: <msg>"` — swap off but
  file remains. Error shows in statusbar for 5 s.
- Both ok → `OpStatus::Done`.

#### New actions

```rust
RequestConfirmOffDelete,
ToggleConfirmDeleteFile,
CancelConfirmOffDelete,
```

#### New `AppState` field

```rust
pub confirm_off_delete: Option<ConfirmOffDelete>,
```

### Files changed

- `src/actions.rs` — `DeviceOpKind::OffAndDelete` + 3 new actions.
- `src/app.rs` — `ConfirmOffDelete` struct, `confirm_off_delete` field,
  reducer arms.
- `src/input.rs` — `handle_devices_key` routes `f` differently for File type;
  new key handler for the delete-confirm modal.
- `src/main.rs` — `OffAndDelete` arm in `spawn_blocking`.
- `src/ui/devices.rs` — `render_off_delete_modal` function.

### No new dependencies.

---

## Area 4 — Inactive Swap Discovery in Device Listing

### Problem

The Devices tab only shows entries from `/proc/swaps` (active swap). Swap
files that exist on disk but are inactive are invisible, making it hard to
activate or manage them.

### Design

Extend `LinuxBackend::swap_devices()` to probe well-known paths after parsing
`/proc/swaps`.

#### Discovery candidates

**Fixed paths** (always checked):
- `/swapfile`
- `/var/swapfile`
- `/swap`
- `/swap.img`

**Block device scan** (`/dev/`):
1. `std::fs::read_dir("/dev/")` — single level, no recursion.
2. For each entry: `nix::sys::stat::stat(path)` → keep only block devices
   (`S_ISBLK`).
3. Skip entries already in the active set from `/proc/swaps`.
4. Skip entries with size < 4096 bytes (can't hold swap header).
5. Read first 4096 bytes → `detect_swap_magic()`.

#### Integration into `swap_devices()`

```rust
pub fn swap_devices(&self) -> Result<Vec<SwapDevice>> {
    let mut devices = self.parse_proc_swaps()?;
    let active_paths: HashSet<PathBuf> = devices.iter()
        .map(|d| d.path.clone())
        .collect();

    // Fixed well-known paths
    for candidate in WELL_KNOWN_SWAP_PATHS {
        let path = PathBuf::from(candidate);
        if active_paths.contains(&path) { continue; }
        if let Some(dev) = self.probe_swap_file(&path) {
            devices.push(dev);
        }
    }

    // Block device scan
    if let Ok(entries) = std::fs::read_dir("/dev/") {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if active_paths.contains(&path) { continue; }
            if !is_block_device(&path) { continue; }
            if let Some(dev) = self.probe_swap_device(&path) {
                devices.push(dev);
            }
        }
    }

    Ok(devices)
}
```

#### Helper: `probe_swap_file`

For regular files at well-known paths:
1. `fs::metadata(path)` — exists? is file?
2. Read first 4096 bytes → `detect_swap_magic(buf, size)`.
3. If magic matches → `SwapDevice { path, total: size, used: 0, priority: 0, kind: SwapKind::File, active: false }`.

#### Helper: `probe_swap_device`

For block devices in `/dev/`:
1. `nix::sys::stat::stat(path)` → `S_ISBLK` check.
2. Open, read 4096 bytes → `detect_swap_magic`.
3. Block device size via `ioctl(BLKGETSIZE64)` or `fs::metadata().len()`.
4. If magic matches → `SwapDevice { kind: SwapKind::Partition, active: false, ... }`.

#### Performance per tick (1 s)

- 4 × `stat()` on fixed paths: ~negligible.
- 1 × `read_dir("/dev/")`: ~0.2 ms (typically 200-400 entries).
- `stat()` per non-active block device: ~negligible.
- 4096-byte reads only for candidates passing the stat check.

Total added latency: < 5 ms per tick — well within the 1 s budget.

#### `SwapDevice` struct

Already has `active: bool` — no changes needed. The UI already renders
`INACTIVE` for `active == false`. The discovery integrates with zero renderer
changes.

### Files changed

- `src/platform/linux.rs` — new `probe_swap_file`, `probe_swap_device`,
  `is_block_device` helpers; extended `swap_devices()`.
- `src/create_swap.rs` — `detect_swap_magic` is already public and reusable.

### No new dependencies.

---

## Rust Best Practices Applied

### Error handling

- All fs operations use `Result` with `?` propagation — never `unwrap()`.
- `probe_swap_file` / `probe_swap_device` return `Option<SwapDevice>` — a
  probe failure (permissions, IO) silently skips the candidate rather than
  crashing the tick.
- `OffAndDelete` background task chains errors with context:
  `"deactivated; delete failed: {e}"` preserves both the success state
  (swap off) and the failure (rm).
- No `String` error types in new public APIs — use existing `OpStatus::Error`
  for display and `Result<(), String>` only within the fire-and-forget
  `spawn_blocking` closures (matching the existing Phase 4 pattern).

### Ownership & borrowing

- `compute_path_completions` takes `&str`, returns owned `Vec<String>` — caller
  owns the allocations, no lifetime entanglement.
- `probe_swap_file` / `probe_swap_device` take `&Path` (borrowed), return
  `Option<SwapDevice>` (owned) — clean transfer at the boundary.
- Completion popup render borrows `&[String]` from modal state — no clones.
- `ConfirmOffDelete.path` is `PathBuf` (owned) — copied once when the modal
  opens, then moved into `ExecuteDeviceOp` on confirm.

### Performance

- No intermediate `.collect()` in discovery loop — filter-map chain feeds
  directly into `devices.push()`.
- `HashSet<PathBuf>` for O(1) active-path lookups instead of linear scan.
- `read_dir` iterator is lazy — entries processed one at a time, no full
  directory materialization.
- `detect_swap_magic` reuses a stack-allocated `[u8; 4096]` buffer — no heap
  allocation per probe.

### Safety

- No new `unsafe` blocks. Existing `unsafe` in `do_swapon` / `do_swapon_with_priority` unchanged.
- `is_block_device` uses `nix::sys::stat::stat` (safe wrapper) — no raw
  syscalls.
- `ioctl(BLKGETSIZE64)` is used only in the `/dev/` probe path and only
  through `nix::ioctl_read!` macro (if needed) — documented safety invariant.

### Testing

- `compute_path_completions` is pure enough to test with a tempdir fixture.
- `probe_swap_file` testable with a crafted 4096-byte file containing magic.
- `ConfirmOffDelete` state transitions testable as pure reducer tests.
- Completion popup renderer testable via `centered_rect`-style unit tests.

---

## Out of scope

- Persistent swap (adding to `/etc/fstab`).
- Recursive path search beyond `/dev/` depth 1.
- Completion for Size/Priority fields.
- Fuzzy path matching (strict prefix only).
