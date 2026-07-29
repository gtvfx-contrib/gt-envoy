use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;

use envoy_core::discovery::{discover_bundles_auto, Bundle};
use envoy_core::stack_registry::publish_stack;
use tempfile::tempdir;

struct EnvVarGuard {
    name: &'static str,
    previous: Option<OsString>,
}

impl EnvVarGuard {
    fn set(name: &'static str, value: &OsStr) -> Self {
        let previous = env::var_os(name);
        env::set_var(name, value);
        Self { name, previous }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(value) => env::set_var(self.name, value),
            None => env::remove_var(self.name),
        }
    }
}

#[test]
fn supports_envoy_utils_discovery_and_stack_contract() {
    let temp = tempdir().expect("failed to create temp dir");
    let bundle_root = temp.path().join("bundles");
    let bundle_path = bundle_root.join("gt").join("contract_fixture");
    fs::create_dir_all(bundle_path.join(".envoy")).expect("failed to create .envoy");
    fs::create_dir_all(bundle_path.join(".git")).expect("failed to create .git");

    let roots = env::join_paths([bundle_root]).expect("failed to join bundle roots");
    let _roots_guard = EnvVarGuard::set("ENVOY_BNDL_ROOTS", roots.as_os_str());

    let discovered = discover_bundles_auto().expect("bundle discovery should succeed");
    assert!(discovered
        .iter()
        .any(|bundle| bundle.bndlid() == "gt:contract_fixture"));

    let bundle = Bundle::new("gt:contract_fixture", None)
        .expect("bundle ID should resolve through ENVOY_BNDL_ROOTS");
    assert_eq!(bundle.bndlid(), "gt:contract_fixture");

    let source_stack = temp.path().join("contract.estack");
    let stack_contents = format!(
        "name: contract\nbundles:\n  - path: '{}'\n",
        bundle_path.display()
    );
    fs::write(&source_stack, stack_contents).expect("failed to write stack");

    let stack_root = temp.path().join("stacks");
    let published = publish_stack(&stack_root, "contract", &source_stack, false)
        .expect("stack publish should succeed");

    assert!(published.is_file());
    assert!(stack_root.join("contract").join("latest").is_file());
}
