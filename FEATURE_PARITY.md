# AgentFS Feature Parity

**Last updated:** 2026-02-11
**Upstream version:** 0.6.0-pre.4

## Core Filesystem (upstream)

| Feature                              |  Status   | Notes                          |
| ------------------------------------ | :-------: | ------------------------------ |
| SQLite inode/dentry/data schema      |   Done    | SPEC.md v0.4                   |
| Chunked file storage (4KB default)   |   Done    | `fs_data` table                |
| Hard links                           |   Done    | `nlink` tracking in `fs_inode` |
| Symbolic links                       |   Done    | `fs_symlink` table             |
| Special files (FIFO, device, socket) |   Done    | `rdev` + mode bits             |
| Nanosecond timestamps                |   Done    | `*_nsec` columns               |
| File permissions (chmod)             |   Done    | Mode bits in `fs_inode`        |
| File ownership (chown)               |   Done    | `uid`/`gid` in `fs_inode`      |
| utimens (atime/mtime)                |   Done    | `TimeChange` enum              |
| Connection pool                      |   Done    | `connection_pool.rs`           |
| Prepared statement cache             |   Done    | v0.5.3                         |
| Dentry cache                         |   Done    | v0.5.1                         |
| **Subtotal**                         | **12/12** | **100%**                       |

## FUSE Surface (Linux)

| Feature                              |  Status   | Notes                                                        |
| ------------------------------------ | :-------: | ------------------------------------------------------------ |
| `init` (capabilities)                |   Done    | async_read, writeback_cache, parallel_dirops, cache_symlinks |
| `lookup`                             |   Done    |                                                              |
| `getattr`                            |   Done    |                                                              |
| `setattr` (chmod, truncate, utimens) |   Done    |                                                              |
| `readdir`                            |   Done    |                                                              |
| `readdirplus`                        |   Done    | Avoids N+1 queries                                           |
| `open` / `release`                   |   Done    | File handle tracking                                         |
| `read` (pread)                       |   Done    |                                                              |
| `write` (pwrite)                     |   Done    | With hook integration                                        |
| `create`                             |   Done    |                                                              |
| `mkdir` / `rmdir`                    |   Done    |                                                              |
| `unlink`                             |   Done    |                                                              |
| `rename`                             |   Done    |                                                              |
| `symlink` / `readlink`               |   Done    |                                                              |
| `link`                               |   Done    | Hard links                                                   |
| `mknod`                              |   Done    | Special files                                                |
| `flush`                              |   Done    | No-op (writes go to DB)                                      |
| `fsync`                              |   Done    | Per-file handle                                              |
| `statfs`                             |   Done    | Reports actual usage                                         |
| `forget` / `batch_forget`            |   Done    | Inode cache lifecycle                                        |
| `chown`                              |   Done    |                                                              |
| **Subtotal**                         | **21/21** | **100%**                                                     |

## NFS Surface (macOS + Linux)

| Feature              | Status  | Notes                            |
| -------------------- | :-----: | -------------------------------- |
| NFS v3 server        |  Done   | `nfsserve` vendored              |
| macOS `mount_nfs`    |  Done   | `mount_nfs -o locallocks,vers=3` |
| Linux `mount -t nfs` |  Done   |                                  |
| Auto port selection  |  Done   | Scans from 11111                 |
| **Subtotal**         | **4/4** | **100%**                         |

## Overlay Filesystem

| Feature                         | Status  | Notes                     |
| ------------------------------- | :-----: | ------------------------- |
| OverlayFS (HostFS + AgentFS)    |  Done   | Copy-on-write delta       |
| Whiteout tracking               |  Done   | `fs_whiteout` table       |
| Inode origin tracking (copy-up) |  Done   | `fs_origin` table         |
| In-memory whiteout cache        |  Done   | v0.5.1                    |
| NormalizedPath type             |  Done   | v0.5.1                    |
| Overlay config persistence      |  Done   | `fs_overlay_config` table |
| `diff` command                  |  Done   | Shows delta vs base       |
| **Subtotal**                    | **7/7** | **100%**                  |

## Sandbox

| Feature                              |    Status    | Notes                         |
| ------------------------------------ | :----------: | ----------------------------- |
| Linux namespace isolation            |     Done     | User + mount namespaces       |
| Default allow list (~/.config, etc.) |     Done     | Configurable via --allow      |
| Session isolation (--session)        |     Done     | Shared delta layer            |
| ptrace sandbox (Reverie)             | Experimental | `--experimental-sandbox` flag |
| macOS sandbox                        | Not started  | No namespace equivalent       |
| **Subtotal**                         |   **3/5**    | **60%**                       |

## Lev Integration

| Feature                                 |   Status    | Notes                                  |
| --------------------------------------- | :---------: | -------------------------------------- |
| `lev-reactive` sync hooks (pre-write)   |    Done     | Fires before FUSE write                |
| `lev-reactive` async hooks (post-write) |    Done     | Fire-and-forget after write            |
| LevFS Validator plugin                  |    Done     | Size, frontmatter, schema              |
| LevFS Workflow plugin                   |    Done     | Flowmind CLI spawn                     |
| Dynamic plugin loading (C ABI)          |    Done     | `create_plugin()` / `_plugin_create()` |
| Hook config from XDG                    |   Partial   | Path defined, loading not wired        |
| Hooks on non-write ops                  | Not started | Only `write()` has hooks               |
| NFS hook support                        | Not started | macOS has no hooks                     |
| ConnectorPort (governed reads)          | Not started | Planned kernel feature                 |
| BindingPort (governed writes)           | Not started | Planned kernel feature                 |
| **Subtotal**                            |  **5/10**   | **50%**                                |

## Serving & Protocols

| Feature                      | Status  | Notes                 |
| ---------------------------- | :-----: | --------------------- |
| MCP server                   |  Done   | Filesystem + KV tools |
| MCP tool filtering (--tools) |  Done   | Selective exposure    |
| NFS server (standalone)      |  Done   | `agentfs serve nfs`   |
| **Subtotal**                 | **3/3** | **100%**              |

## Operational

| Feature                      | Status  | Notes                   |
| ---------------------------- | :-----: | ----------------------- |
| `agentfs ps` (list sessions) |  Done   |                         |
| `agentfs prune mounts`       |  Done   | Linux only              |
| `agentfs timeline`           |  Done   | Table + JSON output     |
| `agentfs diff`               |  Done   | Overlay delta view      |
| `agentfs fs ls/cat/write`    |  Done   | Direct DB access        |
| Shell completions            |  Done   | Install/uninstall/show  |
| Turso cloud sync (pull/push) |  Done   |                         |
| Encryption (aegis/aes)       |  Done   | Multiple cipher options |
| Daemonize (background mount) |  Done   | Linux                   |
| **Subtotal**                 | **9/9** | **100%**                |

## Integrity & Reliability

| Feature                  |   Status    | Notes                            |
| ------------------------ | :---------: | -------------------------------- |
| Data checksums           | Not started | No corruption detection          |
| Scrub / integrity check  | Not started |                                  |
| WAL-based recovery       | Not started | SQLite WAL exists but no tooling |
| Multi-writer concurrency | Not started | SQLite is single-writer          |
| **Subtotal**             |   **0/4**   | **0%**                           |

## Testing

| Feature                              |   Status    | Notes                 |
| ------------------------------------ | :---------: | --------------------- |
| C syscall tests (rename, stat, etc.) |    Done     | `cli/tests/syscall/`  |
| Shell integration tests              |    Done     | `cli/tests/test-*.sh` |
| Benchmark suite (Criterion)          |    Done     | `sdk/rust/benches/`   |
| LevFS validator unit tests           |    Done     | `validator.rs` tests  |
| LevFS workflow unit tests            |    Done     | `workflow.rs` tests   |
| Conformance golden fixtures          | Not started | No golden comparison  |
| **Subtotal**                         |   **5/6**   | **83%**               |

---

## Summary

| Domain                  | Coverage        |
| ----------------------- | --------------- |
| Core Filesystem         | 12/12 (100%)    |
| FUSE Surface            | 21/21 (100%)    |
| NFS Surface             | 4/4 (100%)      |
| Overlay Filesystem      | 7/7 (100%)      |
| Sandbox                 | 3/5 (60%)       |
| Lev Integration         | 5/10 (50%)      |
| Serving & Protocols     | 3/3 (100%)      |
| Operational             | 9/9 (100%)      |
| Integrity & Reliability | 0/4 (0%)        |
| Testing                 | 5/6 (83%)       |
| **Overall**             | **69/81 (85%)** |

---

## Key Gaps (by priority)

| Priority | Gap                         | Impact                                          |
| -------- | --------------------------- | ----------------------------------------------- |
| P1       | Hooks only on `write()`     | Can't gate `create`, `unlink`, `rename`         |
| P1       | NFS path has no hooks       | macOS users get no validation/workflow triggers |
| P2       | No data integrity checking  | Silent corruption possible                      |
| P2       | Single-writer SQLite        | Multi-agent contention bottleneck               |
| P2       | macOS sandbox               | No isolation on macOS                           |
| P3       | Conformance golden fixtures | No regression detection                         |
| P3       | ConnectorPort/BindingPort   | Governed access not implemented                 |
