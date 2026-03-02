use std::fs;
use std::path::Path;
use std::process::Command;

fn main() {
    let spirv_dir = Path::new("shaders/spirv");
    fs::create_dir_all(spirv_dir).expect("create shaders/spirv");

    let shaders = [
        "scene.vert",
        "scene.frag",
        "trail.vert",
        "trail.frag",
        "hud.vert",
        "hud.frag",
    ];

    for name in &shaders {
        let src = format!("shaders/{}", name);
        let dst = format!("shaders/spirv/{}.spv", name);
        println!("cargo:rerun-if-changed={}", src);

        let status = Command::new("glslangValidator")
            .args(["-V", &src, "-o", &dst])
            .status()
            .unwrap_or_else(|e| panic!("Failed to run glslangValidator: {}", e));

        assert!(
            status.success(),
            "glslangValidator failed for {}",
            name
        );
    }
}
