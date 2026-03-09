# LevFS — Leviathan AgentFS Integration Spec

**Version:** 0.1
**Upstream:** [tursodatabase/agentfs](https://github.com/tursodatabase/agentfs) (forked, branch `lev-reactive-integration`)
**Depends on:** `lev-reactive` (workspace crate)

## Overview

LevFS extends upstream AgentFS with Leviathan-specific capabilities. The upstream provides the core filesystem (SQLite-backed FUSE/NFS, overlay, sandbox, KV store, tool call audit). LevFS adds **reactive hooks**, **validation gates**, and **workflow triggers** that fire on filesystem operations.

This document covers **only what Lev adds**. For the core filesystem spec, see [SPEC.md](SPEC.md).

---

## 1. Architecture

```
┌─────────────────────────────────────────────────┐
│                   FUSE / NFS                     │
│              (kernel ↔ userspace)                 │
└───────────────────┬─────────────────────────────┘
                    │
┌───────────────────▼─────────────────────────────┐
│              AgentFSFuse adapter                 │
│  ┌──────────────────────────────────────────┐   │
│  │  Sync Hooks (pre-op)   │ Async Hooks     │   │
│  │  ├─ LevFSValidator     │ (post-op)       │   │
│  │  │  ├─ size check      │ ├─ LevFSWorkflow│   │
│  │  │  ├─ frontmatter     │ │  └─ flowmind  │   │
│  │  │  └─ schema validate │ │     CLI spawn  │   │
│  │  └─ [user hooks]       │ └─ [user hooks]  │   │
│  └──────────────────────────────────────────┘   │
└───────────────────┬─────────────────────────────┘
                    │
┌───────────────────▼─────────────────────────────┐
│           FileSystem trait (SDK)                  │
│  ┌────────────┐ ┌────────────┐ ┌─────────────┐  │
│  │  AgentFS   │ │  OverlayFS │ │   HostFS    │  │
│  │ (SQLite)   │ │ (CoW delta)│ │ (passthru)  │  │
│  └────────────┘ └────────────┘ └─────────────┘  │
└──────────────────────────────────────────────────┘
```

### Layering Rules

- **Hooks are FUSE-layer only.** The `FileSystem` trait and SDK know nothing about hooks.
- **Sync hooks can block operations.** They run before the write and can return `Deny`.
- **Async hooks are fire-and-forget.** They run after the write, in a background task.
- **Hooks are optional.** If no `HookRegistry` is configured, operations pass through unchanged.

---

## 2. Reactive Hooks Integration

LevFS integrates with `lev-reactive` to intercept filesystem operations with configurable hook pipelines.

### 2.1 Hook Execution Points

Currently, hooks fire on **one** FUSE operation:

| Operation | Sync (pre-op) | Async (post-op) |
|-----------|:---:|:---:|
| `write()` | Yes | Yes |
| `read()` | No | No |
| `create()` | No | No |
| `unlink()` | No | No |
| `mkdir()` | No | No |
| `rename()` | No | No |

### 2.2 Hook Context

Every hook receives a `HookContext` with:

```rust
HookContext {
    event_type: "file:write",    // operation identifier
    source: "levfs",             // always "levfs" for filesystem hooks
    data: {
        "fh": u64,               // file handle
        "offset": i64,           // write offset
        "size": usize,           // data length
    }
}
```

### 2.3 Hook Decisions

Sync hooks return one of:

| Decision | Effect |
|---|---|
| `Allow` | Operation proceeds |
| `Deny` | Operation rejected, FUSE returns `EPERM` |
| `AllowWithMessage(_)` | Treated as `Deny` (returns `EPERM`) |
| `Transform(_)` | Treated as `Allow` (transform not yet used) |

On hook error, FUSE returns `EIO`.

### 2.4 Hook Configuration

Hooks are loaded from XDG config at `~/.config/lev/reactive/hooks.yaml`.

```yaml
hooks:
  - name: levfs-validator
    type: sync
    priority: 100
    config:
      max_size: 10485760  # 10MB
      schema_dir: ~/.config/lev/schemas/
  - name: levfs-workflow
    type: async
    priority: 100
    config:
      workflow: default-workflow
```

---

## 3. LevFS Validator Plugin

A sync hook that validates file writes before they reach the filesystem.

**Module:** `cli/src/levfs/validator.rs`
**Hook name:** `levfs-validator`
**Priority:** 100 (high — validates early)
**FFI:** Exports `create_plugin()` / `destroy_plugin()` for dynamic loading

### 3.1 Validation Pipeline

```
write() → size check → frontmatter parse → schema validate → Allow/Block
```

### 3.2 Size Enforcement

| Condition | Decision |
|---|---|
| `size <= 80% of max` | `Allow` |
| `size > 80% of max` | `Warn` (logged, operation proceeds) |
| `size > max` | `Block` (operation rejected) |

Default max: 10MB. Configurable via `with_max_size()`.

### 3.3 Frontmatter Validation

Parses YAML frontmatter from file content (delimited by `---`). If a schema is specified in the hook context metadata (`schema` key), validates required fields against loaded schemas.

**Schema format** (loaded from `~/.config/lev/schemas/<name>.yaml`):

```yaml
name: document
required_fields:
  - title
  - author
optional_fields:
  - tags
  - status
max_size: 5242880
```

### 3.4 Decision Priority

When multiple checks produce decisions, the most severe wins:

```
Block > Warn > Allow
```

---

## 4. LevFS Workflow Plugin

An async hook that spawns Flowmind CLI workflows on filesystem events.

**Module:** `cli/src/levfs/workflow.rs`
**Plugin name:** `levfs-workflow`
**Hook name:** `flowmind-workflow`
**Priority:** 100
**FFI:** Exports `_plugin_create()` for dynamic loading

### 4.1 Execution Model

1. FUSE `write()` completes successfully
2. Async hook fires in background (`tokio::spawn`)
3. Hook serializes `HookContext` to JSON
4. Spawns `flowmind run <workflow-name>` with context on stdin
5. Returns `Allow` immediately (non-blocking)

### 4.2 Error Handling

Workflow failures are logged via `tracing::error` but do **not** affect the filesystem operation (fire-and-forget).

---

## 5. Platform Support Matrix

| Capability | Linux | macOS |
|---|:---:|:---:|
| FUSE mount | Yes | No |
| NFS mount | Yes | Yes |
| Overlay FS | Yes | Yes |
| Sandbox (namespace) | Yes | No |
| Sandbox (ptrace/Reverie) | Yes (experimental) | No |
| MCP server | Yes | Yes |
| Encryption | Yes | Yes |
| Turso cloud sync | Yes | Yes |
| Reactive hooks (FUSE) | Yes | No (NFS path lacks hooks) |

### 5.1 macOS Limitation

Reactive hooks currently only fire through the FUSE code path (`cli/src/fuse.rs`). The NFS adapter does not have hook integration. This means hooks are **Linux-only** until NFS hook support is added.

---

## 6. Storage Layout

### 6.1 Agent Database Location

```
~/.agentfs/<id>.db          # default location
<any-path>.db               # explicit path also supported
```

### 6.2 Overlay Mode

When initialized with `--base <dir>`:

```
Host filesystem (read-only base)
  └── OverlayFS (union mount)
       ├── HostFS: passthrough to real disk
       └── AgentFS: SQLite delta layer
            ├── fs_whiteout: deleted files
            ├── fs_origin: copy-up inode mapping
            └── fs_overlay_config: base_path setting
```

### 6.3 Session Isolation

`agentfs run --session <id>` creates isolated sessions sharing the same base:

```
~/.agentfs/<session-id>.db   # per-session delta
```

---

## 7. MCP Server Tools

When serving via `agentfs serve mcp`, the following tools are exposed:

| Tool | Description |
|---|---|
| `read_file` | Read file contents |
| `write_file` | Write file contents |
| `readdir` | List directory entries |
| `mkdir` | Create directory |
| `rmdir` | Remove directory |
| `rm` / `unlink` | Remove file |
| `copy_file` | Copy a file |
| `rename` | Rename/move file or directory |
| `stat` | Get file metadata |
| `access` | Check file accessibility |
| `kv_get` | Get KV store value |
| `kv_set` | Set KV store value |
| `kv_delete` | Delete KV store key |
| `kv_list` | List KV store keys |

Tools can be selectively exposed via `--tools` flag.

---

## 8. Integration Points with Lev

| System | Integration | Status |
|---|---|---|
| `lev-reactive` | Sync/async hook registry for file operations | Implemented |
| Flowmind | Workflow triggers on file events | Implemented (async hook) |
| `.lev/agentfs/` | Event logging (gather/exec/deploy JSONL) | CLI integration |
| Deploy (`levd`) | Deploy plans, status, rollback via agentfs paths | CLI integration |
| ConnectorPort | Governed reads via capability pattern | Planned |
| BindingPort | Governed writes via capability pattern | Planned |

---

## 9. Gaps and Future Work

| Gap | Description | Priority |
|---|---|---|
| NFS hook support | Hooks only fire through FUSE path; macOS has no hooks | P1 |
| Hook coverage | Only `write()` has hooks; need `create`, `unlink`, `rename`, `mkdir` | P1 |
| ConnectorPort/BindingPort | Governed access pattern from kernel design | P2 |
| Integrity checking | No checksums on stored data; no corruption detection | P2 |
| Multi-agent concurrency | SQLite WAL is single-writer; contention under multi-agent | P2 |
| Hook transform | `Transform` decision is accepted but not acted on | P3 |
| L1-L6 level-of-detail | Per-node shearing layers metadata | P3 |

---

## Revision History

### Version 0.1

- Initial Lev integration spec
- Documents reactive hooks, validator, workflow plugins
- Platform support matrix
- Integration points and gaps
