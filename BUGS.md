# BUGS

Out-of-scope-to-fix-right-now things noticed during work. Each line should have enough context to act on later.

- **PowerShell COM-poller child orphans on Tauri hot-reload.** `src-tauri/src/itunes.rs` spawns `powershell.exe` as a child process. When `pnpm tauri dev` rebuilds and restarts the Rust binary, the child outlives its parent. `tokio::process::Command::kill_on_drop(true)` won't help because the parent is killed externally. Fix: assign the child to a Windows JobObject with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`. Real users (one app run, one child, OS reaps on shutdown) aren't affected; this only matters during dev iteration.
