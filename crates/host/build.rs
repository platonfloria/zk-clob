fn main() {
    let cycle_tracking = std::env::var_os("CARGO_FEATURE_SP1_CYCLE_TRACKING").is_some();
    let output_directory = std::env::var("OUT_DIR").expect("Cargo must provide OUT_DIR");
    let args = sp1_build::BuildArgs {
        elf_name: Some("zk-clob-guest".to_owned()),
        features: cycle_tracking
            .then(|| "sp1-cycle-tracking".to_owned())
            .into_iter()
            .collect(),
        output_directory: Some(output_directory.clone()),
        ..Default::default()
    };

    sp1_build::build_program_with_args("../guest", args);

    // SP1 otherwise exposes the shared target ELF, which is overwritten when
    // switching between feature sets. Point this host build at its immutable
    // feature-specific copy under Cargo's unique OUT_DIR instead.
    println!("cargo:rustc-env=SP1_ELF_zk-clob-guest={output_directory}/zk-clob-guest");
}
