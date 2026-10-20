// Migrations are embedded by `sqlx::migrate!()` at compile time; rebuild when they change.
// JOY_BUILD is a short hash of everything the browser downloads (static/ + templates/), so
// asset URLs and the service worker change exactly when those files do.
use std::{
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash, Hasher},
    path::Path,
};

fn hash_dir(dir: &Path, h: &mut DefaultHasher) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    let mut paths: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();
    for p in paths {
        if p.is_dir() {
            hash_dir(&p, h);
        } else if let Ok(bytes) = fs::read(&p) {
            p.to_string_lossy().hash(h);
            bytes.hash(h);
        }
    }
}

fn main() {
    println!("cargo:rerun-if-changed=migrations");
    println!("cargo:rerun-if-changed=static");
    println!("cargo:rerun-if-changed=templates");
    let mut h = DefaultHasher::new();
    hash_dir(Path::new("static"), &mut h);
    hash_dir(Path::new("templates"), &mut h);
    println!("cargo:rustc-env=JOY_BUILD={:012x}", h.finish() & 0xffff_ffff_ffff);
}
