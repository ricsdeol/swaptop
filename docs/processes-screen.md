# Processes Screen

The Processes screen (tab `2`) shows all running user-space processes with their
memory and CPU usage. Press `2` or `Tab` to reach it.

## Columns

| Column | Description |
|--------|-------------|
| PID    | Process ID |
| Name   | Executable name |
| User   | Owner of the process |
| RSS    | Resident memory in use (RAM) |
| Swap   | Swap space in use (Linux only) |
| CPU%   | Current CPU usage |

The active sort column is marked with `▾` (descending) or `▲` (ascending).

## Navigation

| Key | Action |
|-----|--------|
| `j` / `↓` | Move selection down |
| `k` / `↑` | Move selection up |
| `s` | Cycle sort column (Swap → CPU% → RSS → PID → Name → Swap) |
| `/` | Open filter input |
| `Enter` | Open process detail *(Phase 3)* |

## Filtering

Press `/` to open the filter bar. Type to narrow the list by process name.
Press `Enter` or `Esc` to close the filter bar — the filter text stays active
until you clear it with `Backspace`.

## Platform notes

**Linux:** All columns available. Swap per process is read from `/proc/PID/smaps`.

**macOS / other:** The Swap column shows `—`. Swap per process is not available
without private kernel APIs.
