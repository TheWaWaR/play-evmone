use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let libevmone_file = "libevmone.so";
    if let Some(path) = first_path_with_file(libevmone_file) {
        println!("cargo:rustc-link-search=native={}", path);
        println!("cargo:rustc-link-lib=dylib=evmone");
    } else {
        panic!("Can not find {}", libevmone_file);
    }
}

fn first_path_with_file(file: &str) -> Option<String> {
    // we want to look in LD_LIBRARY_PATH and then some default folders
    if let Some(ld_path) = env::var_os("LD_LIBRARY_PATH") {
        for p in env::split_paths(&ld_path) {
            if is_file_in(file, &p) {
                return p.to_str().map(String::from);
            }
        }
    }
    for p in &["/usr/lib", "/usr/local/lib"] {
        if is_file_in(file, &Path::new(p)) {
            return Some(String::from(*p));
        }
    }
    None
}

fn is_file_in(file: &str, folder: &Path) -> bool {
    let full = folder.join(file);
    match fs::metadata(full) {
        Ok(ref found) if found.is_file() => true,
        _ => false,
    }
}
