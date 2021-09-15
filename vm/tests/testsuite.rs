use anyhow::Result;
use bellman::pairing::bn256::Bn256;
use logger::prelude::*;
use movelang::{argument::ScriptArguments, compiler::compile_script};
use std::fs::File;
use std::io::Read;
use std::path::Path;

pub const TEST_MODULE_PATH: &str = "tests/modules";

#[derive(Debug)]
struct RunConfig {
    args: Option<ScriptArguments>,
    modules: Vec<String>,
}

fn parse_config(script_file: &Path) -> Result<RunConfig> {
    let mut config = RunConfig {
        args: None,
        modules: vec![],
    };
    let file_str = script_file.to_str().expect("path is None.");

    let mut f = File::open(script_file)
        .map_err(|err| std::io::Error::new(err.kind(), format!("{}: {}", err, file_str)))?;
    let mut buffer = String::new();
    f.read_to_string(&mut buffer)?;

    for line in buffer.lines().into_iter() {
        let s = line.split_whitespace().collect::<String>();
        if let Some(s) = s.strip_prefix("//!args:") {
            config.args = Some(s.parse::<ScriptArguments>()?);
        }
        if let Some(s) = s.strip_prefix("//!mods:") {
            config.modules.push(s.to_string()); //todo: support multiple modules
        }
    }
    Ok(config)
}

fn vm_test(path: &Path) -> datatest_stable::Result<()> {
    logger::init_for_test();
    let script_file = path.to_str().expect("path is None.");
    debug!("Run test {:?}", script_file);

    let mut targets = vec![];
    targets.push(script_file.to_string());
    let config = parse_config(path)?;
    for module in config.modules.into_iter() {
        let path = Path::new(TEST_MODULE_PATH)
            .join(module)
            .to_str()
            .unwrap()
            .to_string();
        targets.push(path);
    }
    debug!(
        "script arguments {:?}, compile targets {:?}",
        config.args, targets
    );

    let (compiled_script, compiled_modules) = compile_script(&targets)?;

    if let Some(script) = compiled_script {
        let mut script_bytes = vec![];
        script.serialize(&mut script_bytes)?;
        vm::execute_script(
            script_bytes.clone(),
            compiled_modules.clone(),
            config.args.clone(),
        )?;

        debug!("Generate parameters for script {:?}", script_file);
        let params = vm::setup_script::<Bn256>(script_bytes.clone(), compiled_modules.clone())?;

        debug!("Generate zk proof for script {:?}", script_file);
        let proof = vm::prove_script::<Bn256>(
            script_bytes,
            compiled_modules.clone(),
            config.args,
            &params,
        )?;

        debug!("Verify script {:?}", script_file);
        let success = vm::verify_script::<Bn256>(&params.vk, &proof)?;
        assert_eq!(success, true, "verify failed.");
    }

    Ok(())
}

datatest_stable::harness!(vm_test, "tests/scripts", r".*\.move");
