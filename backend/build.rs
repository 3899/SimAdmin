//! Build script for injecting version and Git information at compile time

fn main() {
    // Read version from VERSION file (default: 3.0.0)
    let version = std::fs::read_to_string("../VERSION")
        .unwrap_or_else(|_| "3.0.0".to_string())
        .trim()
        .to_string();

    // Get Git branch name
    let branch = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    // Get Git commit hash (short)
    let commit = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    // Cargo exposes the selected compilation target to build scripts. Forward it
    // to the application so OTA packages can be checked against the architecture
    // of the running binary instead of a hard-coded target.
    let target = std::env::var("TARGET").unwrap_or_else(|_| "unknown".to_string());

    // Set compile-time environment variables
    println!("cargo:rustc-env=APP_VERSION={}", version);
    println!("cargo:rustc-env=GIT_BRANCH={}", branch);
    println!("cargo:rustc-env=GIT_COMMIT={}", commit);
    println!("cargo:rustc-env=APP_TARGET_TRIPLE={}", target);

    // Rebuild if VERSION file changes
    println!("cargo:rerun-if-changed=../VERSION");
    // Rebuild if git HEAD changes (new commits)
    println!("cargo:rerun-if-changed=../.git/HEAD");
    println!("cargo:rerun-if-changed=../.git/refs/heads/");
}
