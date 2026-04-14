# Devices Tab

The Devices tab (press `3` or navigate with `Tab`) shows all active swap devices and lets you activate, deactivate, or reset them.

## Requirements

Control operations (`o`, `f`, `r`) require root. Run as:

```
sudo swaptop
```

If you run without root, you can still view device status — only control is restricted.

## Columns

| Column | Description |
|--------|-------------|
| Path   | Device path (e.g. `/dev/sda2`, `/swapfile`) |
| Type   | `Partition`, `File`, `Zram`, or `DynamicPager` |
| Total  | Total swap capacity |
| Used   | Currently used swap |
| %      | Usage percentage |
| Pri    | Kernel priority (higher = preferred) |
| Status | `ACTIVE`, `INACTIVE`, `⏳ ...`, `✓ OK`, or `✗ ERROR` |

## Keybindings

| Key         | Action |
|-------------|--------|
| `j` / `↓`  | Move selection down |
| `k` / `↑`  | Move selection up |
| `o`         | Activate selected device (`swapon`) |
| `f`         | Deactivate selected device (`swapoff`) |
| `r`         | Reset selected device (`swapoff` + 100ms + `swapon`) |
| `n`         | Create new swap file (modal wizard, requires root) |
| `s`         | Confirm action (when modal is open) |
| `Esc`       | Cancel confirmation modal |
| `Tab` / `1-3` | Switch tabs |
| `q`         | Quit |

## Status Indicators

- **`ACTIVE`** — device is currently active as swap
- **`INACTIVE`** — device is known but not currently active
- **`⏳ ...`** — operation in progress (swapon/swapoff running)
- **`✓ OK`** — last operation succeeded
- **`✗ ERROR`** — last operation failed (check statusbar for details)

## Reset Operation

Reset (`r`) performs `swapoff` followed by `swapon` with a 100ms pause. This forces the kernel to move all swap pages back to RAM and then re-enable the device, which clears fragmentation. Use it when swap usage is high but actual data could be consolidated.

**Note:** Reset requires enough free RAM to hold all data currently in that swap device. If RAM is too full, `swapoff` will fail with an error.

## Platform Notes

On **macOS**, swap is managed automatically by `dynamic_pager`. The Devices tab shows the active swapfiles but control operations are unavailable.
