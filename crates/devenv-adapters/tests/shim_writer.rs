use devenv_adapters::shim::FileShimWriter;
use devenv_core::{ShimSpec, ShimWriter, ToolName};

#[test]
fn file_shim_writer_writes_dispatch_script() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let spec = ShimSpec::new(ToolName::new("fake").expect("tool should be valid"), "fake");
    let mut writer =
        FileShimWriter::new(temp.path().join("shims")).with_dispatch_command("/opt/devenv");

    writer.write_shim(&spec).expect("shim should be written");

    let shim = temp.path().join("shims/fake");
    let contents = std::fs::read_to_string(&shim).expect("shim should be readable");
    assert!(contents.contains("exec '/opt/devenv' shim dispatch 'fake' -- \"$@\""));

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mode = std::fs::metadata(&shim)
            .expect("metadata should be readable")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o755);
    }
}
