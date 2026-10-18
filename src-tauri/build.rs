use std::{env, path::Path};

fn main() {
    // agent_prompt 的注入防护 / 预算测试只放在本地（tests.rs 被 .gitignore 忽略），
    // CI 上没有这个文件。文件存在时打开 cfg，本地直接 `cargo test` 就会跑；
    // 不存在时 mod.rs 里的 `mod tests;` 不参与编译，CI 照常构建。
    let tests_file = Path::new(&env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"))
        .join("src/application/agent_prompt/tests.rs");
    println!("cargo:rustc-check-cfg=cfg(has_local_agent_prompt_tests)");
    // 监听目录（总是存在）：tauri_build 会输出自己的 rerun-if-changed，
    // 不加这一行的话新建 tests.rs 不会触发 build script 重跑。
    println!("cargo:rerun-if-changed=src/application/agent_prompt");
    if tests_file.exists() {
        println!("cargo:rustc-cfg=has_local_agent_prompt_tests");
    }

    tauri_build::build()
}
