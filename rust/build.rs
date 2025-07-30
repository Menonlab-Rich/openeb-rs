use std::env;
use std::path::PathBuf;
use std::fs;
use regex::Regex;

// A simple struct to parse the compile_commands.json file
#[derive(serde::Deserialize)]
struct CompileCommand {
    command: String,
}

fn main() {
    let project_root = PathBuf::from("..");
    
    // --- CMake Configuration & Build Step ---
    let mut config = cmake::Config::new(&project_root);
    config
        .define("CMAKE_EXPORT_COMPILE_COMMANDS", "ON") // Tell CMake to generate the compile_commands file
        .define("BUILD_SHARED_LIBS", "OFF") // build static libs
        .define("CMAKE_C_COMPILER", "clang")
        .define("CMAKE_CXX_COMPILER", "clang++")
        .define("COMPILE_PYTHON3_BINDINGS", "OFF") // No need to generate new bindings
        .define("BUILD_SAMPLES", "OFF") // No samples in the rust wrapper
        .cxxflag("-include stdint.h"); // Make sure to include stdint in case it's not always done
    
    let targets_to_build = [
        "metavision_hal", "metavision_psee_hw_layer", "metavision_sdk_base",
        "metavision_sdk_core", "metavision_sdk_stream", "metavision_sdk_ui",
    ];

    for target in targets_to_build.iter() {
        config.build_target(target);
    }
    
    let dst = config.build();

    // --- Path Extraction Step ---
    let compile_commands_path = dst.join("build/compile_commands.json");
    let content = fs::read_to_string(compile_commands_path).expect("Could not read compile_commands.json");
    let commands: Vec<CompileCommand> = serde_json::from_str(&content).expect("Failed to parse compile_commands.json");

    // Use a regular expression to find all "-I/path/to/include" arguments
    let re = Regex::new(r"-I([^\s]+)").unwrap();
    let mut all_includes = std::collections::HashSet::new();

    // 
    if let Ok(opencv_path) = env::var("OPENCV_INCLUDE_PATH") {
        all_includes.insert(PathBuf::from(opencv_path));
    }

    all_includes.insert(PathBuf::from("/usr/include/opencv4/"));

    for cmd in commands {
        for cap in re.captures_iter(&cmd.command) {
            all_includes.insert(PathBuf::from(&cap[1]));
        }
    }

    // --- Bindgen Step ---
    let mut builder = bindgen::Builder::default().header("wrapper.h");

    for path in &all_includes {
        builder = builder.clang_arg(format!("-I{}", path.display()));
    }
    
    let bindings = builder
        .clang_arg("-x")
        .clang_arg("c++")
        .clang_arg("-std=c++17")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("Unable to generate bindings");

    bindings
        .write_to_file(PathBuf::from(env::var("OUT_DIR").unwrap()).join("bindings.rs"))
        .expect("Couldn't write bindings!");
        
    // --- Linker Configuration ---
    println!("cargo:rustc-link-search=native={}/build/lib", dst.display());
    
    for target in targets_to_build.iter() {
        println!("cargo:rustc-link-lib=static={}", target);
    }
    println!("cargo:rustc-link-lib=stdc++");
}
