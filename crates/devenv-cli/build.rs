use std::env;

fn main() {
    println!("cargo:rerun-if-env-changed=DEVENV_BUILD_GIT_SHA");
    println!("cargo:rerun-if-env-changed=GITHUB_SHA");

    let target = env::var("TARGET").unwrap_or_else(|_| "unknown".to_owned());
    let profile = env::var("PROFILE").unwrap_or_else(|_| "unknown".to_owned());
    let git_sha = env::var("DEVENV_BUILD_GIT_SHA")
        .or_else(|_| env::var("GITHUB_SHA"))
        .map(|value| short_git_sha(&value))
        .unwrap_or_else(|_| "unknown".to_owned());

    println!("cargo:rustc-env=DEVENV_BUILD_TARGET={target}");
    println!("cargo:rustc-env=DEVENV_BUILD_PROFILE={profile}");
    println!("cargo:rustc-env=DEVENV_BUILD_GIT_SHA={git_sha}");
}

fn short_git_sha(value: &str) -> String {
    let trimmed = value.trim();
    trimmed.chars().take(12).collect()
}
