#[cfg(windows)]
include!("dump_uia_windows.rs");

#[cfg(not(windows))]
fn main() {
    eprintln!("dump_uia is only supported on Windows");
}
