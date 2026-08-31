use openevt::hal::device::discovery::PluginRegistry;
use std::env;
use std::path::PathBuf;

#[test]
fn configured_plugin_path_discovers_the_simulator() {
    let configured_paths: Vec<PathBuf> = match env::var_os("OPENEVT_PLUGIN_PATH") {
        Some(configured_path) => env::split_paths(&configured_path).collect(),
        None => vec![
            env::current_exe()
                .expect("test executable path should be available")
                .parent()
                .expect("test executable should have a parent directory")
                .to_path_buf(),
        ],
    };

    assert!(
        configured_paths.iter().any(|path| path.is_dir()),
        "OPENEVT_PLUGIN_PATH does not contain an existing directory: {:?}",
        configured_paths
    );

    let mut registry = PluginRegistry::new();
    let loaded = configured_paths
        .iter()
        .map(|path| registry.load_directory(path))
        .sum::<u64>();
    assert!(
        loaded > 0,
        "no plugin libraries could be loaded from {:?}",
        configured_paths
    );

    let simulator = registry
        .list_devices()
        .into_iter()
        .find(|device| device.plugin_name == "openevt_simulator")
        .expect("the simulator plugin was loaded but did not advertise its device");

    assert_eq!(simulator.plugin_name, "openevt_simulator");
    assert_eq!(simulator.plugin_info.serial, "EventSimulator");
}
