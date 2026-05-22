// One-shot dev tool: dump the Windows UI Automation tree of every top-level
// window whose process name or window title matches a substring (default:
// "pandora"). Used to discover the AutomationId / Name / ClassName selectors
// that hold the currently-playing track + artist for apps that do NOT publish
// to Windows SMTC (e.g. the Pandora UWP / Microsoft Store app), so we can
// build a `pandora_uwp_bridge.rs` mirror of the existing
// `web_bridge::PandoraProbe` (which targets Pandora-in-Chrome).
//
// Usage (from src-tauri/):
//   cargo run --bin dump_uia                  # pretty tree, needle="pandora"
//   cargo run --bin dump_uia -- --json        # JSON tree
//   cargo run --bin dump_uia -- spotify       # different needle
//   cargo run --bin dump_uia -- pandora --raw # raw-view walker (no filtering)
//
// Why this is non-trivial: when we walk down from the desktop root through
// `walker.get_first_child(...)`, the elements UIA hands back can be cached
// proxies that don't trigger Chromium's accessibility tree to populate. The
// working pattern in `web_bridge::PandoraProbe::read()` is:
//   1. Enumerate top-level HWNDs via EnumWindows + filter by title/process,
//   2. Call `automation.element_from_handle(hwnd)` for each — a FRESH UIA
//      query against that HWND that kicks Chromium / WebView2 / XAML to
//      expose its content subtree.
//   3. Walk with the *control* view (or *raw* view via --raw) from that
//      freshly-anchored element.
//
// NOT wired into the main `hum` binary. Cargo auto-discovers binaries in
// `src/bin/` so no Cargo.toml change is needed.

use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use std::env;
use uiautomation::{UIAutomation, UIElement, UITreeWalker};
use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
};

// Hard caps so a runaway tree (Chrome with 500 tabs, etc.) can't hang the tool.
const MAX_NODES_PER_WINDOW: usize = 20_000;
const MAX_DEPTH: usize = 60;

#[derive(Clone, Copy, PartialEq, Eq)]
enum WalkerMode {
    Control,
    Raw,
}

fn main() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print_help();
        return Ok(());
    }
    let json_mode = args.iter().any(|a| a == "--json");
    let walker_mode = if args.iter().any(|a| a == "--raw") {
        WalkerMode::Raw
    } else {
        WalkerMode::Control
    };
    let needle = args
        .iter()
        .find(|a| !a.starts_with("--"))
        .cloned()
        .unwrap_or_else(|| "pandora".to_string());
    let needle_lc = needle.to_lowercase();

    let automation =
        UIAutomation::new().map_err(|e| anyhow!("UIAutomation::new failed: {e:?}"))?;
    let walker = match walker_mode {
        WalkerMode::Control => automation
            .get_control_view_walker()
            .map_err(|e| anyhow!("get_control_view_walker failed: {e:?}"))?,
        WalkerMode::Raw => automation
            .get_raw_view_walker()
            .map_err(|e| anyhow!("get_raw_view_walker failed: {e:?}"))?,
    };

    let matched_hwnds = find_matching_windows(&needle_lc);
    eprintln!(
        "[dump_uia] needle={:?}  walker={}  matched windows={}",
        needle,
        match walker_mode {
            WalkerMode::Control => "control-view",
            WalkerMode::Raw => "raw-view",
        },
        matched_hwnds.len()
    );
    if matched_hwnds.is_empty() {
        eprintln!(
            "[dump_uia] no matches — is the target app open and visible? Try a different needle, e.g. `cargo run --bin dump_uia -- spotify`."
        );
        if json_mode {
            println!("[]");
        }
        return Ok(());
    }

    let mut json_windows: Vec<Value> = Vec::new();

    for win in &matched_hwnds {
        // Fresh-anchor through the HWND — this is what triggers Chromium /
        // WebView2 / XAML hosts to populate their accessibility subtree.
        let root = match automation.element_from_handle((win.hwnd.0 as isize).into()) {
            Ok(elem) => elem,
            Err(e) => {
                eprintln!(
                    "[dump_uia] element_from_handle failed for hwnd={:?} title={:?}: {e:?}",
                    win.hwnd.0, win.title
                );
                continue;
            }
        };

        if json_mode {
            let mut counter = 0usize;
            json_windows.push(json!({
                "window_title": win.title,
                "process": win.process_name,
                "pid": win.pid,
                "hwnd": win.hwnd.0 as isize,
                "root_class": root.get_classname().unwrap_or_default(),
                "root_name": root.get_name().unwrap_or_default(),
                "tree": build_node_json(&walker, &root, 0, &mut counter),
                "nodes_visited": counter,
            }));
        } else {
            println!(
                "==== Window: title={:?}  process={:?}  pid={}  hwnd=0x{:x} ====",
                win.title, win.process_name, win.pid, win.hwnd.0 as isize
            );
            let mut counter = 0usize;
            walk_tree_pretty(&walker, &root, &[], &mut counter);
            println!("[dump_uia] {} nodes visited in this window", counter);
            println!();
        }
    }

    if json_mode {
        println!("{}", serde_json::to_string_pretty(&Value::Array(json_windows))?);
    }

    Ok(())
}

fn print_help() {
    println!(
        "dump_uia — dump the UI Automation tree of top-level windows matching a needle.\n\
        \n\
        Usage:\n  \
          cargo run --bin dump_uia [-- [<needle>] [--json] [--raw]]\n\
        \n\
        Default needle: \"pandora\" (case-insensitive substring on window title\n\
        or process file name). Use any visible-substring like \"spotify\", \"tidal\",\n\
        \"amazon music\".\n\
        \n\
        Flags:\n  \
          --json    Emit a JSON array instead of an ASCII tree.\n  \
          --raw     Use the raw-view walker (shows every element, including\n            non-content shells). Default is control-view, which is what\n            web_bridge::PandoraProbe uses and reliably triggers Chromium\n            accessibility-tree population.\n  \
          -h, --help    Show this message."
    );
}

// ---- top-level HWND enumeration ----

#[derive(Clone)]
struct MatchedWindow {
    hwnd: HWND,
    title: String,
    process_name: String,
    pid: u32,
}

/// Enumerate visible top-level windows and return those whose title or process
/// file name contains `needle_lc` (already lowercased).
fn find_matching_windows(needle_lc: &str) -> Vec<MatchedWindow> {
    struct Ctx<'a> {
        needle_lc: &'a str,
        hits: Vec<MatchedWindow>,
    }

    let mut ctx = Ctx {
        needle_lc,
        hits: Vec::new(),
    };

    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        // SAFETY: lparam was set to a valid &mut Ctx by the EnumWindows
        // caller. The reference outlives the synchronous EnumWindows call.
        let ctx = unsafe { &mut *(lparam.0 as *mut Ctx) };

        if !unsafe { IsWindowVisible(hwnd) }.as_bool() {
            return BOOL(1);
        }

        let title = read_window_title(hwnd);
        let process_name = read_process_name_for_window(hwnd);
        let title_lc = title.to_lowercase();
        let proc_lc = process_name.to_lowercase();
        if !title_lc.contains(ctx.needle_lc) && !proc_lc.contains(ctx.needle_lc) {
            return BOOL(1);
        }

        let mut pid: u32 = 0;
        let _ = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };

        ctx.hits.push(MatchedWindow {
            hwnd,
            title,
            process_name,
            pid,
        });
        BOOL(1)
    }

    let ctx_ptr: *mut Ctx = &mut ctx;
    let _ = unsafe { EnumWindows(Some(enum_proc), LPARAM(ctx_ptr as isize)) };
    ctx.hits
}

fn read_window_title(hwnd: HWND) -> String {
    let mut buf = [0u16; 512];
    let n = unsafe { GetWindowTextW(hwnd, &mut buf) };
    if n <= 0 {
        return String::new();
    }
    String::from_utf16_lossy(&buf[..n as usize])
}

fn read_process_name_for_window(hwnd: HWND) -> String {
    let mut pid: u32 = 0;
    let _ = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid == 0 {
        return String::new();
    }
    let handle = match unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) } {
        Ok(h) => h,
        Err(_) => return String::new(),
    };
    let mut path_buf = [0u16; 1024];
    let mut size: u32 = path_buf.len() as u32;
    let res = unsafe {
        QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_FORMAT(0),
            windows::core::PWSTR(path_buf.as_mut_ptr()),
            &mut size,
        )
    };
    let _ = unsafe { windows::Win32::Foundation::CloseHandle(handle) };
    if res.is_err() {
        return String::new();
    }
    let full = String::from_utf16_lossy(&path_buf[..size as usize]);
    full.rsplit(['\\', '/']).next().unwrap_or(&full).to_string()
}

// ---- pretty tree printer ----

fn walk_tree_pretty(
    walker: &UITreeWalker,
    elem: &UIElement,
    last_at: &[bool],
    counter: &mut usize,
) {
    if *counter >= MAX_NODES_PER_WINDOW {
        if *counter == MAX_NODES_PER_WINDOW {
            println!(
                "{}… (truncated — hit MAX_NODES_PER_WINDOW={})",
                build_prefix(last_at),
                MAX_NODES_PER_WINDOW
            );
        }
        *counter += 1;
        return;
    }
    *counter += 1;

    let prefix = build_prefix(last_at);
    println!("{prefix}{}", format_elem_oneline(elem));

    if last_at.len() >= MAX_DEPTH {
        println!(
            "{}… (truncated — hit MAX_DEPTH={})",
            build_prefix_for_continuation(last_at),
            MAX_DEPTH
        );
        return;
    }

    let kids = collect_children(walker, elem);
    let last_idx = kids.len().saturating_sub(1);
    for (i, k) in kids.iter().enumerate() {
        let mut next_last = last_at.to_vec();
        next_last.push(i == last_idx);
        walk_tree_pretty(walker, k, &next_last, counter);
    }
}

fn collect_children(walker: &UITreeWalker, parent: &UIElement) -> Vec<UIElement> {
    let mut kids = Vec::new();
    if let Ok(first) = walker.get_first_child(parent) {
        let mut cur = Some(first);
        while let Some(c) = cur {
            kids.push(c.clone());
            cur = walker.get_next_sibling(&c).ok();
        }
    }
    kids
}

fn build_prefix(last_at: &[bool]) -> String {
    let mut s = String::new();
    if last_at.is_empty() {
        return s;
    }
    for is_last in &last_at[..last_at.len() - 1] {
        s.push_str(if *is_last { "    " } else { "│   " });
    }
    s.push_str(if *last_at.last().unwrap() {
        "└── "
    } else {
        "├── "
    });
    s
}

fn build_prefix_for_continuation(last_at: &[bool]) -> String {
    let mut s = String::new();
    for is_last in last_at {
        s.push_str(if *is_last { "    " } else { "│   " });
    }
    s.push_str("└── ");
    s
}

fn format_elem_oneline(elem: &UIElement) -> String {
    let name = elem.get_name().unwrap_or_default();
    let aid = elem.get_automation_id().unwrap_or_default();
    let cls = elem.get_classname().unwrap_or_default();
    let ct = elem
        .get_control_type()
        .map(|c| format!("{c:?}"))
        .unwrap_or_else(|_| "?".into());
    let val = read_value_pattern(elem).unwrap_or_default();
    let lct = elem.get_localized_control_type().unwrap_or_default();

    let mut parts = vec![format!("[{ct}]")];
    if !lct.is_empty() && lct.to_lowercase() != ct.to_lowercase() {
        parts.push(format!("LCT={lct:?}"));
    }
    if !name.is_empty() {
        parts.push(format!("Name={name:?}"));
    }
    if !aid.is_empty() {
        parts.push(format!("AutomationId={aid:?}"));
    }
    if !cls.is_empty() {
        parts.push(format!("Class={cls:?}"));
    }
    if !val.is_empty() {
        parts.push(format!("Value={val:?}"));
    }
    parts.join("  ")
}

fn read_value_pattern(elem: &UIElement) -> Option<String> {
    use uiautomation::patterns::UIValuePattern;
    let p = elem.get_pattern::<UIValuePattern>().ok()?;
    let v = p.get_value().ok()?;
    if v.is_empty() {
        None
    } else {
        Some(v)
    }
}

// ---- JSON tree builder ----

fn build_node_json(
    walker: &UITreeWalker,
    elem: &UIElement,
    depth: usize,
    counter: &mut usize,
) -> Value {
    if *counter >= MAX_NODES_PER_WINDOW {
        if *counter == MAX_NODES_PER_WINDOW {
            *counter += 1;
            return json!({ "truncated": format!("hit MAX_NODES_PER_WINDOW={MAX_NODES_PER_WINDOW}") });
        }
        *counter += 1;
        return Value::Null;
    }
    *counter += 1;

    let name = elem.get_name().unwrap_or_default();
    let aid = elem.get_automation_id().unwrap_or_default();
    let cls = elem.get_classname().unwrap_or_default();
    let ct = elem
        .get_control_type()
        .map(|c| format!("{c:?}"))
        .unwrap_or_else(|_| "?".into());
    let lct = elem.get_localized_control_type().unwrap_or_default();
    let val = read_value_pattern(elem).unwrap_or_default();

    let children: Vec<Value> = if depth >= MAX_DEPTH {
        vec![json!({ "truncated": format!("hit MAX_DEPTH={MAX_DEPTH}") })]
    } else {
        collect_children(walker, elem)
            .iter()
            .map(|c| build_node_json(walker, c, depth + 1, counter))
            .collect()
    };

    json!({
        "control_type": ct,
        "localized_control_type": lct,
        "name": name,
        "automation_id": aid,
        "class": cls,
        "value": val,
        "children": children,
    })
}
