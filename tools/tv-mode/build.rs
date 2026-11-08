use std::{env, path::PathBuf, process::Command};
fn main() {
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let layers = pkg_config::Config::new()
        .probe("gtk4-layer-shell-0")
        .expect("GTK 4 Layer Shell development files are required");
    pkg_config::Config::new()
        .probe("gtk4")
        .expect("GTK 4 development files are required");
    cc::Build::new()
        .file("qt/transition.c")
        .includes(layers.include_paths)
        .compile("tv_transition");
    let qt = pkg_config::Config::new()
        .probe("Qt6Quick")
        .expect("Qt 6 Quick development files are required");
    let gl = pkg_config::Config::new()
        .probe("Qt6OpenGL")
        .expect("Qt 6 OpenGL development files are required");
    let mpv = pkg_config::Config::new()
        .atleast_version("2.3.0")
        .probe("mpv")
        .expect("libmpv development files are required");
    let libexec = Command::new("pkg-config")
        .args(["--variable=libexecdir", "Qt6Core"])
        .output()
        .unwrap();
    let libexec = String::from_utf8(libexec.stdout).unwrap();
    for (tool, args) in [
        (
            "moc",
            vec![
                "qt/bridge.cpp".into(),
                "-o".into(),
                out.join("bridge.moc").display().to_string(),
            ],
        ),
        (
            "rcc",
            vec![
                "--name".into(),
                "resources".into(),
                "qml/resources.qrc".into(),
                "-o".into(),
                out.join("resources.cpp").display().to_string(),
            ],
        ),
    ] {
        assert!(
            Command::new(format!("{}/{tool}", libexec.trim()))
                .args(args)
                .status()
                .unwrap()
                .success(),
            "Qt {tool} failed"
        );
    }
    let mut build = cc::Build::new();
    build.cpp(true).std("c++17").flag("-fPIC").include(&out);
    for path in qt
        .include_paths
        .into_iter()
        .chain(gl.include_paths)
        .chain(mpv.include_paths)
    {
        build.include(path);
    }
    build
        .file("qt/bridge.cpp")
        .file(out.join("resources.cpp"))
        .compile("tv_quick");
    println!("cargo:rerun-if-changed=qt");
    println!("cargo:rerun-if-changed=qml");
}
