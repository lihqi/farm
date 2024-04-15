use std::{
  cell::RefCell,
  collections::{HashMap, HashSet},
  hash::Hash,
  path::PathBuf,
  rc::Rc,
  sync::Arc,
};

use farmfe_core::{
  config::minify::MinifyOptions,
  context::CompilationContext,
  enhanced_magic_string::{bundle::Bundle, types::SourceMapOptions},
  error::{CompilationError, Result},
  module::{module_graph::ModuleGraph, ModuleId, ModuleType},
  resource::resource_pot::{ResourcePot, ResourcePotId},
  swc_common::{comments::SingleThreadedComments, Mark},
  swc_ecma_ast::Id,
  swc_ecma_parser::EsConfig,
};
use farmfe_toolkit::{
  common::{build_source_map, create_swc_source_map, Source},
  minify::minify_js_module,
  script::{codegen_module, parse_module, swc_try_with::try_with, CodeGenCommentsConfig},
  swc_ecma_transforms::{
    feature::enable_available_feature_from_es_version,
    fixer,
    helpers::inject_helpers,
    hygiene::hygiene_with_config,
    hygiene::Config as HygieneConfig,
    modules::{
      common_js,
      import_analysis::import_analyzer,
      util::{Config, ImportInterop},
    },
    resolver,
  },
  swc_ecma_visit::VisitMutWith,
};

pub use crate::resource_pot_to_bundle::bundle_analyzer::BundleAnalyzer;

use self::{
  bundle::ModuleAnalyzerManager, modules_analyzer::module_analyzer::ModuleAnalyzer,
  uniq_name::BundleVariable,
};

mod bundle;
mod common;
mod defined_idents_collector;
mod modules_analyzer;
mod process_ast;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Var {
  var: Id,
  rename: Option<String>,
  removed: bool,
}

impl Var {
  pub fn new(id: Id) -> Self {
    Var {
      var: id,
      ..Default::default()
    }
  }

  pub fn render_name(&self) -> String {
    if let Some(rename) = self.rename.as_ref() {
      rename.clone()
    } else {
      self.var.0.to_string()
    }
  }

  pub fn origin_name(&self) -> String {
    self.var.0.to_string()
  }
}

impl Hash for Var {
  fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
    self.var.hash(state);
  }
}

mod bundle_analyzer;
mod bundle_external;

mod uniq_name;

pub struct SharedBundle<'a> {
  pub bundle_map: HashMap<ResourcePotId, BundleAnalyzer<'a>>,
  module_analyzer_manager: ModuleAnalyzerManager,
  module_graph: &'a ModuleGraph,
  context: &'a Arc<CompilationContext>,
  bundle_variables: Rc<RefCell<BundleVariable>>,
}

impl<'a> SharedBundle<'a> {
  pub fn new(
    resource_pots: &'a Vec<&mut ResourcePot>,
    module_graph: &'a ModuleGraph,
    context: &'a Arc<CompilationContext>,
  ) -> Self {
    let mut module_analyzer_map: HashMap<ModuleId, ModuleAnalyzer> = HashMap::new();
    let mut bundle_map: HashMap<ResourcePotId, BundleAnalyzer> = HashMap::new();

    let bundle_variables = Rc::new(RefCell::new(BundleVariable::new()));
    for resource_pot in resource_pots {
      let mut bundle_analyzer = BundleAnalyzer::new(
        &resource_pot,
        &module_graph,
        &context,
        bundle_variables.clone(),
      );

      bundle_variables
        .borrow_mut()
        .with_namespace(resource_pot.id.clone(), |_| {
          for module_id in resource_pot.modules() {
            let is_dynamic = module_graph.is_dynamic(module_id);
            let is_entry = resource_pot
              .entry_module
              .as_ref()
              .is_some_and(|item| item == module_id);
            let is_runtime = module_graph
              .module(module_id)
              .is_some_and(|m| matches!(m.module_type, ModuleType::Runtime));
            let module = module_graph.module(module_id).unwrap();

            module_analyzer_map.insert(
              module_id.clone(),
              ModuleAnalyzer::new(
                module,
                &context,
                resource_pot.id.clone(),
                is_entry,
                is_dynamic,
                is_runtime,
              )
              .unwrap(),
            );
          }

          bundle_analyzer.build_module_order();

          bundle_map.insert(resource_pot.id.clone(), bundle_analyzer);
        });
    }

    let module_analyzer_manager = ModuleAnalyzerManager::new(module_analyzer_map);

    Self {
      module_analyzer_manager,
      bundle_map,
      module_graph,
      context,
      bundle_variables,
    }
  }

  fn extract_modules(&mut self) -> Result<()> {
    for resource_pot_id in self
      .module_analyzer_manager
      .module_map
      .values()
      .map(|item| item.resource_pot_id.clone())
      .collect::<HashSet<_>>()
    {
      let bundle = self
        .bundle_map
        .get_mut(&resource_pot_id)
        .map(Ok)
        .unwrap_or_else(|| {
          Err(CompilationError::GenericError(format!(
            "fetch unknown resource pot {:?} failed",
            resource_pot_id
          )))
        })?;

      bundle
        .bundle_variable
        .borrow_mut()
        .set_namespace(resource_pot_id);

      self.module_analyzer_manager.extract_modules_statements(
        &bundle.ordered_modules,
        &self.module_graph,
        bundle.bundle_variable.borrow_mut(),
      )?;
    }

    Ok(())
  }

  fn link_modules(&mut self) -> Result<()> {
    self
      .module_analyzer_manager
      .link(&mut self.bundle_variables.borrow_mut(), &self.context);

    Ok(())
  }

  fn render_bundle(&mut self) -> Result<()> {
    for bundle_analyzer in self.bundle_map.values_mut() {
      // println!("// bundle_analyzer: {:?}", bundle_analyzer.resource_pot.id);

      bundle_analyzer
        .bundle_variable
        .borrow_mut()
        .set_namespace(bundle_analyzer.resource_pot.id.clone());

      let used_name = self
        .module_analyzer_manager
        .namespace_uniq_named
        .values()
        .map(|item| bundle_analyzer.bundle_variable.borrow().name(item.0))
        .collect::<Vec<_>>();

      bundle_analyzer
        .bundle_variable
        .borrow_mut()
        .extend_used_name(used_name);

      bundle_analyzer.render(&mut self.module_analyzer_manager)?;
    }

    Ok(())
  }

  pub fn render(&mut self) -> Result<()> {
    self.extract_modules()?;

    self.link_modules()?;

    self.render_bundle()?;

    Ok(())
  }

  pub fn codegen(&mut self, resource_pot_id: &String) -> Result<Bundle> {
    let bundle = self.bundle_map.get_mut(resource_pot_id).unwrap();

    let bundle = bundle.codegen(&mut self.module_analyzer_manager)?;

    Ok(bundle)
  }
}

// pub fn resource_pot_to_bundle<'a>(
//   resource_pot: &'a ResourcePot,
//   module_graph: &'a ModuleGraph,
//   context: &'a Arc<CompilationContext>,
// ) -> Result<BundleAnalyzer<'a>> {
//   let mut bundle_analyzer = BundleAnalyzer::new(resource_pot, module_graph, context);

//   bundle_analyzer.build_module_order();
//   // bundle_analyzer.process_modules()?;
//   // bundle_analyzer.execute_bundle_actions()?;

//   Ok(bundle_analyzer)
// }

// pub fn resource_pot_to_bundle_string(
//   resource_pot: &ResourcePot,
//   module_graph: &ModuleGraph,
//   context: &Arc<CompilationContext>,
// ) -> Result<(Arc<String>, Vec<Arc<String>>)> {
//   let mut bundle_analyzer = BundleAnalyzer::new(resource_pot, module_graph, context);

//   bundle_analyzer.build_module_order();
//   bundle_analyzer.process_modules()?;
//   bundle_analyzer.execute_bundle_actions()?;

//   let bundle = bundle_analyzer.codegen()?;

//   let mut source_map_chains: Vec<Arc<String>> = vec![];
//   let sourcemap_enabled = context.config.sourcemap.enabled(resource_pot.immutable);

//   if sourcemap_enabled {
//     let root = context.config.root.clone();
//     let src_map = bundle
//       .generate_map(SourceMapOptions {
//         include_content: Some(true),
//         remap_source: Some(Box::new(move |src| {
//           format!("/{}", farmfe_utils::relative(&root, src)).to_string()
//         })),
//         ..Default::default()
//       })
//       .map_err(|_| CompilationError::GenerateSourceMapError {
//         id: resource_pot.id.to_string(),
//       })?;
//     let mut buf = vec![];
//     src_map.to_writer(&mut buf).unwrap();

//     source_map_chains.push(Arc::new(String::from_utf8(buf).unwrap()));
//   }

//   let bundle_code = bundle.to_string();

//   let bundle_code = Arc::new(bundle_code);

//   if !context.config.minify.enabled() {
//     Ok((bundle_code, source_map_chains))
//   } else {
//     let minify_options = context
//       .config
//       .minify
//       .clone()
//       .map(|val| MinifyOptions::from(val))
//       .unwrap_or_default();

//     let (cm, _) = create_swc_source_map(Source {
//       path: PathBuf::from(resource_pot.id.as_str()),
//       content: bundle_code.clone(),
//     });

//     let mut bundle_module = parse_module(
//       &resource_pot.id,
//       &bundle_code,
//       farmfe_core::swc_ecma_parser::Syntax::Es(EsConfig::default()),
//       Default::default(),
//     )?;

//     try_with(cm.clone(), &context.meta.script.globals, || {
//       let unresolved_mark = Mark::new();
//       let top_level_mark = Mark::new();

//       bundle_module
//         .ast
//         .visit_mut_with(&mut resolver(unresolved_mark, top_level_mark, false));

//       bundle_module
//         .ast
//         .visit_mut_with(&mut import_analyzer(ImportInterop::Swc, true));
//       bundle_module
//         .ast
//         .visit_mut_with(&mut inject_helpers(unresolved_mark));
//       bundle_module
//         .ast
//         .visit_mut_with(&mut common_js::<&SingleThreadedComments>(
//           unresolved_mark,
//           Config {
//             ignore_dynamic: true,
//             preserve_import_meta: true,
//             ..Default::default()
//           },
//           enable_available_feature_from_es_version(context.config.script.target),
//           Some(&bundle_module.comments),
//         ));

//       bundle_module
//         .ast
//         .visit_mut_with(&mut hygiene_with_config(HygieneConfig {
//           top_level_mark,
//           ..Default::default()
//         }));

//       minify_js_module(
//         &mut bundle_module.ast,
//         cm.clone(),
//         &bundle_module.comments,
//         unresolved_mark,
//         top_level_mark,
//         &minify_options,
//       );

//       bundle_module
//         .ast
//         .visit_mut_with(&mut fixer(Some(&bundle_module.comments)));
//     })?;

//     let mut src_map = vec![];
//     let bundle_minify_byte = codegen_module(
//       &bundle_module.ast,
//       context.config.script.target,
//       cm.clone(),
//       if sourcemap_enabled {
//         Some(&mut src_map)
//       } else {
//         None
//       },
//       true,
//       Some(CodeGenCommentsConfig {
//         comments: &bundle_module.comments,
//         config: &context.config.comments,
//       }),
//     )
//     .unwrap();

//     if sourcemap_enabled {
//       let map = build_source_map(cm, &src_map);
//       let mut buf = vec![];
//       map.to_writer(&mut buf).expect("failed to write sourcemap");

//       source_map_chains.push(Arc::new(String::from_utf8(buf).unwrap()));
//     }

//     let minify_bundle = Arc::new(String::from_utf8(bundle_minify_byte).unwrap());

//     Ok((minify_bundle, source_map_chains))
//   }
// }
