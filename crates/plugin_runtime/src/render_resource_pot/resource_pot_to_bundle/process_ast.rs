use std::{path::PathBuf, sync::Arc};

use farmfe_core::{
  config::minify::{MinifyMode, MinifyOptions},
  context::CompilationContext,
  error::Result,
  module::{module_graph::ModuleGraph, Module, ModuleId, ModuleSystem},
  swc_common::{comments::SingleThreadedComments, Mark, SourceMap},
  swc_ecma_ast::Module as EcmaAstModule,
};
use farmfe_toolkit::{
  common::{create_swc_source_map, Source},
  minify::minify_js_module,
  script::{
    codegen_module,
    swc_try_with::{resolve_module_mark, try_with},
  },
  swc_ecma_transforms::{
    feature::enable_available_feature_from_es_version,
    fixer::paren_remover,
    helpers::inject_helpers,
    hygiene::{hygiene_with_config, Config as HygieneConfig},
    modules::{
      common_js,
      import_analysis::import_analyzer,
      util::{Config, ImportInterop},
    },
  },
  swc_ecma_transforms_base::fixer::fixer,
  swc_ecma_visit::VisitMutWith,
};

use crate::render_resource_pot::source_replacer::{ExistingCommonJsRequireVisitor, SourceReplacer};

pub fn process_ast(
  module_id: &ModuleId,
  ast: &mut EcmaAstModule,
  context: &Arc<CompilationContext>,
  module_graph: &ModuleGraph,
  minify_options: &MinifyOptions,
  is_enabled_minify: impl Fn(&ModuleId) -> bool,
) -> Result<Arc<SourceMap>> {
  let module = module_graph.module(module_id).unwrap();
  let comments: SingleThreadedComments = module.meta.as_script().comments.clone().into();
  let minify_enabled = is_enabled_minify(&module.id);
  // let mut ast = module.meta.as_script().ast.clone();
  let (cm, _) = create_swc_source_map(Source {
    path: PathBuf::from(module_id.resolved_path_with_query(&context.config.root)),
    content: module.content.clone(),
  });

  try_with(cm.clone(), &context.meta.script.globals, || {
    let (unresolved_mark, top_level_mark) = if module.meta.as_script().unresolved_mark == 0
      && module.meta.as_script().top_level_mark == 0
    {
      resolve_module_mark(ast, module.module_type.is_typescript(), context)
    } else {
      let unresolved_mark = Mark::from_u32(module.meta.as_script().unresolved_mark);
      let top_level_mark = Mark::from_u32(module.meta.as_script().top_level_mark);
      (unresolved_mark, top_level_mark)
    };

    // replace commonjs require('./xxx') to require('./xxx', true)
    if matches!(
      module.meta.as_script().module_system,
      ModuleSystem::CommonJs | ModuleSystem::Hybrid
    ) {
      ast.visit_mut_with(&mut ExistingCommonJsRequireVisitor::new(
        unresolved_mark,
        top_level_mark,
      ));
    }

    ast.visit_mut_with(&mut paren_remover(Some(&comments)));

    // ESM to commonjs, then commonjs to farm's runtime module systems
    if matches!(
      module.meta.as_script().module_system,
      ModuleSystem::EsModule | ModuleSystem::Hybrid
    ) {
      ast.visit_mut_with(&mut import_analyzer(ImportInterop::Swc, true));
      ast.visit_mut_with(&mut inject_helpers(unresolved_mark));
      ast.visit_mut_with(&mut common_js::<&SingleThreadedComments>(
        unresolved_mark,
        Config {
          ignore_dynamic: true,
          preserve_import_meta: true,
          ..Default::default()
        },
        enable_available_feature_from_es_version(context.config.script.target),
        Some(&comments),
      ));
    }

    // replace import source with module id
    let mut source_replacer = SourceReplacer::new(
      unresolved_mark,
      top_level_mark,
      module_graph,
      module.id.clone(),
      context.config.mode.clone(),
    );
    ast.visit_mut_with(&mut source_replacer);
    ast.visit_mut_with(&mut hygiene_with_config(HygieneConfig {
      top_level_mark,
      ..Default::default()
    }));

    // wrap_function(&mut cloned_module, unresolved_mark);
    if minify_enabled {
      minify_js_module(
        ast,
        cm.clone(),
        &comments,
        unresolved_mark,
        top_level_mark,
        &minify_options,
      );
    }

    ast.visit_mut_with(&mut fixer(Some(&comments)));
  })?;

  Ok(cm)
}
