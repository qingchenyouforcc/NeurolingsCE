fn main() {
    // unrar C++ 源码依赖注册表 API（advapi32）。
    #[cfg(windows)]
    println!("cargo:rustc-link-lib=advapi32");
}
