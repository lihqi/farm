use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;

use common::generate_runtime;
use farmfe_compiler::Compiler;
use farmfe_core::config::config_regex::ConfigRegex;
use farmfe_core::config::persistent_cache::PersistentCacheConfig;
use farmfe_core::config::{preset_env::PresetEnvConfig, Config, Mode, SourcemapConfig};
use farmfe_core::plugin::UpdateType;
use farmfe_testing_helpers::fixture;

mod common;

fn create_update_compiler(
  input: HashMap<String, String>,
  cwd: PathBuf,
  crate_path: PathBuf,
  minify: bool,
) -> Compiler {
  let compiler = Compiler::new(
    Config {
      input,
      root: cwd.to_string_lossy().to_string(),
      runtime: generate_runtime(crate_path),
      output: farmfe_core::config::OutputConfig {
        filename: "[resourceName].[ext]".to_string(),
        ..Default::default()
      },
      mode: Mode::Development,
      external: vec![
        ConfigRegex::new("^react-refresh$"),
        ConfigRegex::new("^module$"),
      ],
      sourcemap: SourcemapConfig::Bool(false),
      lazy_compilation: false,
      minify,
      preset_env: Box::new(PresetEnvConfig::Bool(false)),
      persistent_cache: Box::new(PersistentCacheConfig::Bool(false)),
      ..Default::default()
    },
    vec![],
  )
  .unwrap();

  compiler
}

fn asset_update_result_code(
  cwd: PathBuf,
  result: &farmfe_core::plugin::UpdateResult,
  name: Option<&str>,
) {
  let mutable_code = &result.mutable_resources;
  let immutable_code = &result.immutable_resources;

  let output = cwd.join(name.unwrap_or("").to_string() + ".output.js");
  let code = format!("{}\n{}", mutable_code, immutable_code);
  if !output.exists() {
    std::fs::write(output, code).unwrap();
  } else {
    if std::env::var("FARM_UPDATE_SNAPSHOTS").is_ok() {
      std::fs::write(output, code).unwrap();
    } else {
      let expected_code = std::fs::read_to_string(output).unwrap();
      // assert lines are the same
      let expected_lines = expected_code.trim().lines().collect::<Vec<&str>>();
      let result_lines = code.trim().lines().collect::<Vec<&str>>();

      for (expected, result) in expected_lines.iter().zip(result_lines.iter()) {
        assert_eq!(expected.trim(), result.trim()); // ignore whitespace
      }

      assert_eq!(expected_lines.len(), result_lines.len());
    }
  }
}

#[test]
fn update_without_dependencies_change() {
  fixture!(
    "tests/fixtures/update/basic/index.html",
    |file, crate_path| {
      let cwd = file.parent().unwrap().to_path_buf();
      let compiler = create_update_compiler(
        HashMap::from([("index".to_string(), "./index.html".to_string())]),
        cwd.clone(),
        crate_path,
        false,
      );

      compiler.compile().unwrap();

      let update_file = file
        .parent()
        .unwrap()
        .join("index.ts")
        .to_string_lossy()
        .to_string();
      let result = compiler
        .update(vec![(update_file, UpdateType::Updated)], || {}, true)
        .unwrap();

      assert_eq!(result.added_module_ids.len(), 0);
      assert_eq!(result.updated_module_ids, vec!["index.ts".into()]);
      assert_eq!(result.removed_module_ids.len(), 0);

      asset_update_result_code(cwd, &result, Some("update0"));
    }
  );
}

#[test]
fn update_without_dependencies_change_css() {
  fixture!(
    "tests/fixtures/update/basic/index.html",
    |file, crate_path| {
      let cwd = file.parent().unwrap().to_path_buf();
      let compiler = create_update_compiler(
        HashMap::from([("index".to_string(), "./index.html".to_string())]),
        cwd.clone(),
        crate_path,
        false,
      );

      compiler.compile().unwrap();

      let update_file = file
        .parent()
        .unwrap()
        .join("index.css")
        .to_string_lossy()
        .to_string();
      let result = compiler
        .update(
          vec![(update_file.clone(), UpdateType::Updated)],
          || {},
          true,
        )
        .unwrap();

      assert_eq!(result.added_module_ids.len(), 0);
      assert_eq!(result.updated_module_ids, vec!["index.css".into()]);
      assert_eq!(result.removed_module_ids.len(), 0);

      asset_update_result_code(cwd.clone(), &result, Some("update1"));

      let result = compiler
        .update(vec![(update_file, UpdateType::Updated)], || {}, false)
        .unwrap();

      assert_eq!(result.added_module_ids.len(), 0);
      assert_eq!(result.updated_module_ids, vec!["index.css".into()]);
      assert_eq!(result.removed_module_ids.len(), 0);

      asset_update_result_code(cwd, &result, Some("update2"));
    }
  );
}

#[test]
fn update_with_dependencies_change_css_modules() {
  fixture!(
    "tests/fixtures/update/css-modules/index.html",
    |file, crate_path| {
      let cwd = file.parent().unwrap().to_path_buf();
      let compiler = create_update_compiler(
        HashMap::from([("index".to_string(), "./index.html".to_string())]),
        cwd.clone(),
        crate_path,
        false,
      );

      compiler.compile().unwrap();

      let update_file = file
        .parent()
        .unwrap()
        .join("index.ts")
        .to_string_lossy()
        .to_string();
      // read original index.ts
      let mut original_ts = String::new();
      let mut original_ts_file = File::open(&update_file).unwrap();
      original_ts_file.read_to_string(&mut original_ts).unwrap();
      let mut original_ts_file = File::create(&update_file).unwrap();
      // update index.ts
      original_ts_file
        .write_all(
          original_ts
            .replace(
              "console.log('Hello, world!');",
              "import styles from './index.module.css';\nconsole.log(styles);",
            )
            .as_bytes(),
        )
        .unwrap();

      let result = compiler
        .update(
          vec![(update_file.clone(), UpdateType::Updated)],
          || {},
          true,
        )
        .unwrap();
      assert_eq!(result.added_module_ids.len(), 2,);
      assert!(result.added_module_ids.contains(&"index.module.css".into()));
      assert!(result
        .added_module_ids
        .contains(&"index.module.css.FARM_CSS_MODULES?f1d5b6cc".into()));
      assert_eq!(result.updated_module_ids, vec!["index.ts".into()]);
      assert_eq!(result.removed_module_ids.len(), 0);

      asset_update_result_code(cwd.clone(), &result, Some("update1"));

      let update_file_css = cwd.join("index.module.css").to_string_lossy().to_string();
      // read original index.module.css
      let mut original_css = String::new();
      let mut original_css_file = File::open(&update_file_css).unwrap();
      original_css_file.read_to_string(&mut original_css).unwrap();
      // update index.module.css
      let mut original_css_file = File::create(&update_file_css).unwrap();
      original_css_file
        .write_all(original_css.replace(".className", ".className2").as_bytes())
        .unwrap();
      let result = compiler
        .update(
          vec![(update_file_css.clone(), UpdateType::Updated)],
          || {},
          true,
        )
        .unwrap();
      assert_eq!(
        result.added_module_ids,
        vec!["index.module.css.FARM_CSS_MODULES?b2914899".into()]
      );
      assert_eq!(result.updated_module_ids, vec!["index.module.css".into()]);
      assert_eq!(
        result.removed_module_ids,
        vec!["index.module.css.FARM_CSS_MODULES?f1d5b6cc".into()]
      );
      // restore index.module.css
      let mut original_css_file = File::create(&update_file_css).unwrap();
      original_css_file
        .write_all(original_css.as_bytes())
        .unwrap();

      // restore index.ts
      let mut original_ts_file = File::create(&update_file).unwrap();
      original_ts_file.write_all(original_ts.as_bytes()).unwrap();
      let result = compiler
        .update(vec![(update_file, UpdateType::Updated)], || {}, false)
        .unwrap();

      assert_eq!(result.added_module_ids.len(), 0);
      assert_eq!(result.updated_module_ids, vec!["index.ts".into()]);
      assert_eq!(result.removed_module_ids.len(), 2);
      assert!(result
        .removed_module_ids
        .contains(&"index.module.css".into()));
      assert!(result
        .removed_module_ids
        .contains(&"index.module.css.FARM_CSS_MODULES?b2914899".into()));
      asset_update_result_code(cwd, &result, Some("update2"));
    }
  );
}
