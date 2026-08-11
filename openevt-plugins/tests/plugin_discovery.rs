use openevt::hal::device::discovery::PluginRegistry;
use std::env;

#[test]
fn configured_plugin_path_discovers_the_simulator() {
    let configured_path = env::var_os("OPENEVT_PLUGIN_PATH")
        .expect("OPENEVT_PLUGIN_PATH must point to the built plugin directory");
    let configured_paths: Vec<_> = env::split_paths(&configured_path).collect();

    assert!(
        configured_paths.iter().any(|path| path.is_dir()),
        "OPENEVT_PLUGIN_PATH does not contain an existing directory: {:?}",
        configured_paths
    );

    let mut registry = PluginRegistry::new();
    let loaded = registry.load_default_paths();
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
