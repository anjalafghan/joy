// Migrations are embedded by `sqlx::migrate!()` at compile time; rebuild when they change.
fn main() {
    println!("cargo:rerun-if-changed=migrations");
}
