use std::{
  cell::RefCell,
  cmp::Ordering,
  collections::{HashMap, HashSet},
  rc::Rc,
  sync::Arc,
};

use farmfe_core::{
  config::Mode,
  context::CompilationContext,
  enhanced_magic_string::{
    bundle::{Bundle, BundleOptions},
    magic_string::{MagicString, MagicStringOptions},
  },
  error::{CompilationError, Result},
  module::{module_graph::ModuleGraph, ModuleId},
  resource::resource_pot::ResourcePot,
  swc_common::{comments::SingleThreadedComments, util::take::Take, DUMMY_SP},
  swc_ecma_ast::{
    ExportAll, ExportDefaultDecl, ExportDefaultExpr, ExportNamedSpecifier,
    ExportNamespaceSpecifier, ExportSpecifier, Expr, ImportDecl, ImportDefaultSpecifier,
    ImportNamedSpecifier, ImportStarAsSpecifier, ModuleDecl, ModuleItem, NamedExport, Str,
  },
};
use farmfe_toolkit::{
  common::build_source_map,
  script::{codegen_module, swc_try_with::try_with, CodeGenCommentsConfig},
  swc_ecma_transforms::fixer,
  swc_ecma_visit::VisitMutWith,
};

use crate::resource_pot_to_bundle::modules_analyzer::module_analyzer::{
  ExportSpecifierInfo, Variable,
};

// mod tests {
//   #[test]
//   fn test() {
//     assert_eq!(otr!(Some(1), "failed"), Ok(1));
//     assert_eq!(otr!(Option::<usize>::None, "failed"), Err("failed"));
//   }
// }

use super::{
  bundle::ModuleAnalyzerManager,
  bundle_external::{BundleReference, ExternalReferenceExport},
  common,
  modules_analyzer::{
    module_analyzer::{ImportSpecifierInfo, StmtAction},
    ModuleAction, ModulesAnalyzer,
  },
  uniq_name::{BundleVariable, FindModuleExportResult},
};

fn generate_export(
  source: Option<&ModuleId>,
  export: &ExternalReferenceExport,
  bundle_variable: &BundleVariable,
) -> Vec<ModuleItem> {
  let mut stmts = vec![];

  let mut specifiers = vec![];

  let source = source.map(|source| source.relative_path());
  for (exported, local) in export.named.iter() {
    if bundle_variable.var_by_index(*local).removed {
      continue;
    }

    let named_render_name = bundle_variable.render_name(*local);
    let exported_name = bundle_variable.name(*exported);

    let exported_name = if named_render_name == exported_name {
      None
    } else {
      Some(exported_name.as_str().into())
    };

    specifiers.push(farmfe_core::swc_ecma_ast::ExportSpecifier::Named(
      ExportNamedSpecifier {
        span: DUMMY_SP,
        orig: farmfe_core::swc_ecma_ast::ModuleExportName::Ident(named_render_name.as_str().into()),
        exported: exported_name.map(farmfe_core::swc_ecma_ast::ModuleExportName::Ident),
        is_type_only: false,
      },
    ));
  }

  if let Some(namespace) = export.namespace.as_ref() {
    specifiers.push(farmfe_core::swc_ecma_ast::ExportSpecifier::Namespace(
      ExportNamespaceSpecifier {
        span: DUMMY_SP,
        name: farmfe_core::swc_ecma_ast::ModuleExportName::Ident(
          bundle_variable.name(*namespace).as_str().into(),
        ),
      },
    ));
  }

  if let Some(source) = source {
    if export.all {
      stmts.push(ModuleItem::ModuleDecl(ModuleDecl::ExportAll(ExportAll {
        span: DUMMY_SP,
        src: Box::new(source.into()),
        type_only: false,
        with: None,
      })));
    }
  }

  if !specifiers.is_empty() {
    stmts.push(ModuleItem::ModuleDecl(ModuleDecl::ExportNamed(
      NamedExport {
        span: DUMMY_SP,
        specifiers,
        src: source.map(|source| Box::new(source.into())),
        type_only: false,
        with: None,
      },
    )));
  }

  if let Some(source) = source {
    if let Some(_) = export.default.as_ref() {
      stmts.push(ModuleItem::ModuleDecl(ModuleDecl::ExportAll(ExportAll {
        span: DUMMY_SP,
        src: Box::new(source.into()),
        type_only: false,
        with: None,
      })));
    }
  }

  stmts
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum BundleAction {
  SaveImport(ModuleId),
  SaveExport(ModuleId),
}

enum NamespaceExportType {
  External,
  Bundle,
  Entry,
}

pub struct BundleAnalyzer<'a> {
  // pub modules_analyzer: ModulesAnalyzer,
  toposort_order: HashMap<ModuleId, usize>,
  bundle_modules: HashSet<&'a ModuleId>,

  pub resource_pot: &'a ResourcePot,
  pub ordered_modules: Vec<&'a ModuleId>,
  pub bundle_variable: Rc<RefCell<BundleVariable>>,

  module_graph: &'a ModuleGraph,
  context: Arc<CompilationContext>,

  pub bundle_external_reference: BundleReference,

  pub actions: HashSet<BundleAction>,
}

impl<'a> BundleAnalyzer<'a> {
  pub fn new(
    resource_pot: &'a ResourcePot,
    module_graph: &'a ModuleGraph,
    context: &Arc<CompilationContext>,
    bundle_variable: Rc<RefCell<BundleVariable>>,
  ) -> Self {
    Self {
      bundle_variable,
      // modules_analyzer: ModulesAnalyzer::new(),
      toposort_order: HashMap::new(),
      resource_pot,
      ordered_modules: vec![],
      module_graph,
      context: context.clone(),
      bundle_modules: HashSet::new(),
      bundle_external_reference: BundleReference::new(),
      actions: HashSet::new(),
    }
  }

  // step: 1 toposort fetch modules
  pub fn build_module_order(&mut self) {
    let (toposort_modules, _circles) = self.module_graph.toposort();

    let order_map = &mut self.toposort_order;
    let mut toposort_modules_set = HashSet::new();
    let mut resource_pot_modules = self.resource_pot.modules();

    for (index, module_id) in toposort_modules.into_iter().enumerate() {
      order_map.insert(module_id.clone(), index);
      toposort_modules_set.insert(module_id);
    }

    resource_pot_modules.sort_by(|a, b| {
      if !order_map.contains_key(a) || !order_map.contains_key(b) {
        return Ordering::Greater;
      }

      return order_map[*b].cmp(&order_map[*a]);
    });

    self.bundle_modules = resource_pot_modules.clone().into_iter().collect();
    self.ordered_modules = resource_pot_modules;
  }

  pub fn module_conflict_name(&mut self, module_analyzer_manager: &mut ModuleAnalyzerManager) {
    for module_id in &self.ordered_modules {
      if let Some(module_analyzer) = module_analyzer_manager.module_analyzer_mut(&module_id) {
        let variables = module_analyzer.variables(&self.bundle_variable.borrow());

        for index in variables {
          self.bundle_variable.borrow_mut().set_var_uniq_rename(index);
        }
      };
    }
  }

  pub fn strip_module(
    &mut self,
    module_analyzer_manager: &mut ModuleAnalyzerManager,
  ) -> Result<()> {
    for module_id in &self.ordered_modules {
      if let Some(module_analyzer) = module_analyzer_manager.module_analyzer_mut(module_id) {
        let mut stmt_action = HashSet::new();

        for statement in &module_analyzer.statements {
          if let Some(_) = statement.import.as_ref() {
            stmt_action.insert(StmtAction::RemoveImport(statement.id));
          }

          if let Some(export) = statement.export.as_ref() {
            if export.source.is_none() {
              for specify in &export.specifiers {
                match specify {
                  ExportSpecifierInfo::All(_) => {
                    stmt_action.insert(StmtAction::StripExport(statement.id));
                  }

                  ExportSpecifierInfo::Named { .. } => {
                    stmt_action.insert(StmtAction::StripExport(statement.id));
                  }

                  ExportSpecifierInfo::Default(default) => {
                    if self.bundle_variable.borrow().name(*default) == "default" {
                      stmt_action.insert(StmtAction::DeclDefaultExpr(statement.id, *default));
                    } else {
                      stmt_action.insert(StmtAction::StripImport(statement.id));
                    }
                  }

                  ExportSpecifierInfo::Namespace(_) => {
                    unreachable!("unsupported namespace have't source")
                  }
                }
              }
            } else {
              stmt_action.insert(StmtAction::StripExport(statement.id));
            }
          }
        }
        module_analyzer.statement_actions.extend(stmt_action);
      }
    }

    Ok(())
  }

  pub fn analyzer_module_relation(
    &mut self,
    module_analyzer_manager: &mut ModuleAnalyzerManager,
  ) -> Result<()> {
    for module_id in &self.ordered_modules {
      if module_analyzer_manager.is_in_namespace(module_id) {
        if let Some((local, named_as)) =
          module_analyzer_manager.namespace_uniq_named.get(&module_id)
        {
          let resource_pot_id = module_analyzer_manager
            .module_analyzer(module_id)
            .map(|item| item.resource_pot_id.clone())
            .unwrap();

          let issuer = self.module_graph.dependents_ids(module_id);

          if let Some(result) = issuer.iter().find_map(|item| {
            if let Some(m) = module_analyzer_manager.module_analyzer(item) {
              if m.resource_pot_id != resource_pot_id {
                return Some(NamespaceExportType::Bundle);
              }

              if m.entry {
                return Some(NamespaceExportType::Entry);
              }

              if m.external {
                return Some(NamespaceExportType::External);
              }
            }

            return None;
          }) {
            match result {
              NamespaceExportType::External => {
                self
                  .bundle_external_reference
                  .sync_export(&ExportSpecifierInfo::Named(local.clone().into()), &None);
              }
              NamespaceExportType::Bundle => {
                self
                  .bundle_external_reference
                  .sync_export(&ExportSpecifierInfo::Named(local.clone().into()), &None);
              }
              NamespaceExportType::Entry => {
                self.bundle_external_reference.sync_export(
                  &ExportSpecifierInfo::Named((*local, Some(*named_as)).into()),
                  &None,
                );
              }
            }
          }
        };
      }

      if let Some(module_analyzer) = module_analyzer_manager.module_analyzer(module_id) {
        let is_entry = module_analyzer.entry;
        let resource_pot_id = module_analyzer.resource_pot_id.clone();

        for statement in &module_analyzer.statements {
          if let Some(import) = &statement.import {
            for specify in &import.specifiers {
              match specify {
                // import * as fs from "node:fs"
                ImportSpecifierInfo::Namespace(ns) => {
                  let target = self.bundle_variable.borrow().find_ident_by_index(
                    *ns,
                    &import.source,
                    &module_analyzer_manager.module_map,
                    resource_pot_id.clone(),
                    &self.module_graph,
                    false,
                    true,
                  );

                  if let Some(target) = target {
                    match target {
                      FindModuleExportResult::Local(_, _) => {
                        let common_import_rename = self.bundle_external_reference.sync_import(
                          &import.source,
                          specify,
                          &self.bundle_variable.borrow(),
                        )?;
                        let rendered_name = self
                          .bundle_variable
                          .borrow()
                          .render_name(common_import_rename);

                        self
                          .bundle_variable
                          .borrow_mut()
                          .set_rename_force(*ns, rendered_name);
                      }
                      FindModuleExportResult::External(_, _) => {
                        self.bundle_external_reference.sync_import(
                          &import.source,
                          specify,
                          &self.bundle_variable.borrow(),
                        )?;

                        let rename = self.bundle_variable.borrow().name(
                          common::otr!(
                            module_analyzer_manager
                              .namespace_uniq_named
                              .get(&import.source),
                            CompilationError::GenericError(
                              "failed to fetch namespace uniq name".to_string()
                            )
                          )?
                          .0,
                        );

                        self
                          .bundle_variable
                          .borrow_mut()
                          .set_rename_force(*ns, rename);
                      }
                      FindModuleExportResult::Bundle(_, bundle_name) => {
                        let common_import_rename = self.bundle_external_reference.sync_import(
                          &ModuleId::from(bundle_name),
                          specify,
                          &self.bundle_variable.borrow(),
                        )?;

                        let rendered_name = self
                          .bundle_variable
                          .borrow()
                          .render_name(common_import_rename);

                        self
                          .bundle_variable
                          .borrow_mut()
                          .set_rename_force(*ns, rendered_name);
                      }
                    }
                  }
                  // if let Some(source) = module_analyzer_manager.module_analyzer(&import.source) {
                  //   if source.resource_pot_id != resource_pot_id {
                  //     self.bundle_external_reference.sync_export(
                  //       &ExportSpecifierInfo::Named((*ns).into()),
                  //       &Some(source.resource_pot_id.clone().into()),
                  //     );
                  //   }
                  // };
                  // let rename = self.bundle_variable.borrow().name(
                  //   common::otr!(
                  //     module_analyzer_manager
                  //       .namespace_uniq_named
                  //       .get(&import.source),
                  //     CompilationError::GenericError(
                  //       "failed to fetch namespace uniq name".to_string()
                  //     )
                  //   )?
                  //   .0,
                  // );

                  // println!("import namespace: {}", rename);
                }

                ImportSpecifierInfo::Named { local, imported } => {
                  let imported = imported.unwrap_or(*local);
                  let target = self.bundle_variable.borrow().find_ident_by_index(
                    imported,
                    &import.source,
                    &module_analyzer_manager.module_map,
                    resource_pot_id.clone(),
                    self.module_graph,
                    self.bundle_variable.borrow().name(imported) == "default",
                    false,
                  );

                  if let Some(target) = target {
                    match target {
                      FindModuleExportResult::Local(index, _) => {
                        let rendered_name = self.bundle_variable.borrow().render_name(index);
                        self
                          .bundle_variable
                          .borrow_mut()
                          .set_rename_force(*local, rendered_name);
                      }
                      FindModuleExportResult::External(_, target) => {
                        let rename = self.bundle_external_reference.sync_import(
                          &target,
                          specify,
                          &self.bundle_variable.borrow(),
                        )?;
                        // external
                        let rendered_name = self.bundle_variable.borrow().render_name(rename);
                        self
                          .bundle_variable
                          .borrow_mut()
                          .set_rename_force(*local, rendered_name);
                      }
                      FindModuleExportResult::Bundle(_, target) => {
                        let rename = self.bundle_external_reference.sync_import(
                          &target,
                          specify,
                          &self.bundle_variable.borrow(),
                        )?;

                        let rendered_name = self.bundle_variable.borrow().render_name(rename);
                        self
                          .bundle_variable
                          .borrow_mut()
                          .set_rename_force(*local, rendered_name);
                      }
                    }
                  };
                }

                ImportSpecifierInfo::Default(default) => {
                  let target = self.bundle_variable.borrow().find_ident_by_index(
                    *default,
                    &import.source,
                    &module_analyzer_manager.module_map,
                    resource_pot_id.clone(),
                    self.module_graph,
                    true,
                    false,
                  );

                  if let Some(target) = target {
                    match target {
                      FindModuleExportResult::Local(index, _) => {
                        let rendered_name = self.bundle_variable.borrow().render_name(index);
                        self
                          .bundle_variable
                          .borrow_mut()
                          .set_rename_force(*default, rendered_name);
                      }
                      FindModuleExportResult::External(index, target) => {
                        let rename = self.bundle_external_reference.sync_import(
                          &target,
                          specify,
                          &self.bundle_variable.borrow(),
                        )?;

                        println!(
                          "default {}\ntarget: {}",
                          self.bundle_variable.borrow().name(*default),
                          self.bundle_variable.borrow().name(index)
                        );

                        // external
                        let rendered_name = self.bundle_variable.borrow().render_name(rename);
                        self
                          .bundle_variable
                          .borrow_mut()
                          .set_rename_force(*default, rendered_name);
                      }

                      FindModuleExportResult::Bundle(_, target) => {
                        let rename = self.bundle_external_reference.sync_import(
                          &target,
                          specify,
                          &self.bundle_variable.borrow(),
                        )?;

                        let rendered_name = self.bundle_variable.borrow().render_name(rename);
                        self
                          .bundle_variable
                          .borrow_mut()
                          .set_rename_force(*default, rendered_name);
                      }
                    }
                  };
                }
              }
            }
          }

          if let Some(export) = &statement.export {
            for specify in &export.specifiers {
              match specify {
                // export * from 'module'
                ExportSpecifierInfo::All(_) => {
                  if let Some(source) = &export.source {
                    if !module_analyzer.is_runtime {
                      let exports = module_analyzer_manager.export_names(source);

                      for (export, export_source) in exports {
                        let is_in_self_bundle = self.ordered_modules.contains(&&export_source);

                        for specify in export.specifiers {
                          let t = Some(export.source.clone().unwrap_or(export_source.clone()));
                          self.bundle_external_reference.sync_export(
                            &specify,
                            if is_in_self_bundle {
                              &export.source
                            } else {
                              &t
                            },
                          );
                        }
                      }
                    }
                  }
                }

                // export { a as b }
                // export { a as b } from './module';
                ExportSpecifierInfo::Named(variables) => {
                  if let Some(source) = &export.source {
                    let is_find_default =
                      self.bundle_variable.borrow().name(variables.local()) == "default";
                    let target = self.bundle_variable.borrow_mut().find_ident_by_index(
                      variables.export_as(),
                      source,
                      &module_analyzer_manager.module_map,
                      resource_pot_id.clone(),
                      self.module_graph,
                      is_find_default,
                      false,
                    );

                    if let Some(target) = target {
                      match target {
                        FindModuleExportResult::Local(_, _) => {
                          self.bundle_external_reference.sync_export(specify, &None);
                        }
                        FindModuleExportResult::External(_, target) => {
                          self
                            .bundle_external_reference
                            .sync_export(specify, &Some(target));
                        }
                        FindModuleExportResult::Bundle(_, _) => {}
                      }
                    }
                  } else {
                  }
                }

                // export default n, Default(n)
                // export default 1 + 1, Default(default)
                ExportSpecifierInfo::Default(var) => {
                  if self.bundle_variable.borrow().name(*var) == "default" {
                    self
                      .bundle_variable
                      .borrow_mut()
                      .fetch_module_safe_name_and_set_var_rename(*var, &module_id, &self.context);
                  }
                }

                // export * as ns from 'module'
                ExportSpecifierInfo::Namespace(ns) => {
                  let source = common::otr!(
                    export.source.as_ref(),
                    CompilationError::GenericError(
                      "namespace should have source, but not found".to_string()
                    )
                  )?;

                  let (local_var, _) = module_analyzer_manager
                    .namespace_uniq_named
                    .get(source)
                    .unwrap();

                  let local_name = self.bundle_variable.borrow().name(*local_var);

                  self
                    .bundle_variable
                    .borrow_mut()
                    .set_rename(*ns, local_name);

                  // bundle
                  if let Some(module_analyzer) = module_analyzer_manager.module_analyzer(source) {
                    // export * from './other_bundle_module'
                    if module_analyzer.resource_pot_id != resource_pot_id {
                      // export { } from './other_bundle_module'
                      self.bundle_external_reference.sync_export(
                        &ExportSpecifierInfo::Named((*ns).into()),
                        &Some(module_analyzer.resource_pot_id.clone().into()),
                      );
                    }
                  }
                  // external
                  else {
                    // export * as fs from "node:fs" => import * as node_fs from "node:fs"
                    self.bundle_external_reference.sync_import(
                      source,
                      &ImportSpecifierInfo::Namespace(*ns),
                      &self.bundle_variable.borrow(),
                    )?;
                  }
                }
              }
            }
          }
        }
      }
    }

    // println!(
    //   "bundle_external_reference: {:#?}",
    //   self.bundle_external_reference,
    // );

    Ok(())
  }

  pub fn render(&mut self, module_analyzer_manager: &mut ModuleAnalyzerManager) -> Result<()> {
    self.module_conflict_name(module_analyzer_manager);

    self.strip_module(module_analyzer_manager)?;

    self.analyzer_module_relation(module_analyzer_manager)?;

    self.patch_ast(module_analyzer_manager)?;

    Ok(())
  }

  pub fn patch_ast(&mut self, module_analyzer_manager: &mut ModuleAnalyzerManager) -> Result<()> {
    for module_id in &self.ordered_modules {
      module_analyzer_manager.patch_module_analyzer_ast(
        &module_id,
        &self.context,
        &mut self.bundle_variable.borrow_mut(),
        &mut self.bundle_external_reference,
      )?;
    }

    let mut patch_to_last_module = vec![];
    let mut patch_to_first_module = vec![];

    for (source, export) in self.bundle_external_reference.external_export_map.iter() {
      patch_to_last_module.extend(generate_export(
        Some(source),
        export,
        &self.bundle_variable.borrow(),
      ));
    }

    if let Some(export) = self.bundle_external_reference.export.as_ref() {
      patch_to_last_module.extend(generate_export(
        None,
        export,
        &self.bundle_variable.borrow(),
      ));
    }

    for (source, import) in &self.bundle_external_reference.import_map {
      if import.named.is_empty() && import.namespace.is_none() && import.default.is_none() {
        continue;
      }

      let mut specifiers = vec![];

      for (imported, local) in import.named.iter() {
        let local_named = self.bundle_variable.borrow().render_name(*local);

        specifiers.push(farmfe_core::swc_ecma_ast::ImportSpecifier::Named(
          ImportNamedSpecifier {
            span: DUMMY_SP,
            local: local_named.as_str().into(),
            imported: if imported == &local_named {
              None
            } else {
              Some(farmfe_core::swc_ecma_ast::ModuleExportName::Ident(
                imported.as_str().into(),
              ))
            },
            is_type_only: false,
          },
        ));
      }

      if let Some(namespace) = import.namespace.as_ref() {
        specifiers.push(farmfe_core::swc_ecma_ast::ImportSpecifier::Namespace(
          ImportStarAsSpecifier {
            span: DUMMY_SP,
            local: self
              .bundle_variable
              .borrow()
              .render_name(*namespace)
              .as_str()
              .into(),
          },
        ));
      }

      if let Some(default) = import.default.as_ref() {
        specifiers.push(farmfe_core::swc_ecma_ast::ImportSpecifier::Default(
          ImportDefaultSpecifier {
            span: DUMMY_SP,
            local: self
              .bundle_variable
              .borrow()
              .render_name(*default)
              .as_str()
              .into(),
          },
        ));
      }

      patch_to_first_module.push(ModuleItem::ModuleDecl(ModuleDecl::Import(ImportDecl {
        span: DUMMY_SP,
        specifiers,
        src: Box::new(Str {
          span: DUMMY_SP,
          value: source.as_str().into(),
          raw: None,
        }),
        type_only: false,
        with: None,
        phase: farmfe_core::swc_ecma_ast::ImportPhase::Evaluation,
      })));
    }

    if !patch_to_first_module.is_empty() {
      if let Some(module_analyzer) = self
        .ordered_modules
        .first()
        .map(|item| module_analyzer_manager.module_analyzer_mut(item))
        .flatten()
      {
        let ast = &mut module_analyzer.ast;

        ast.body = patch_to_first_module
          .into_iter()
          .chain(ast.body.take().into_iter())
          .collect();
      };
    }

    if !patch_to_last_module.is_empty() {
      if let Some(module_analyzer) = self
        .ordered_modules
        .last()
        .map(|id| module_analyzer_manager.module_analyzer_mut(id))
        .flatten()
      {
        let ast = &mut module_analyzer.ast;

        ast.body = ast
          .body
          .take()
          .into_iter()
          .chain(patch_to_last_module)
          .collect();
      };
    }

    Ok(())
  }

  // // step: 2 process modules
  // pub fn process_modules(&mut self) -> Result<()> {
  //   for module_id in &self.bundle_ordered_modules {
  //     // step: 2
  //     // self.modules_analyzer.extract_module_statement(
  //     //   module_id,
  //     //   &self.module_graph,
  //     //   &mut self.bundle_variable,
  //     //   &self.context,
  //     // )?;

  //     // step: 3 analyze import and export
  //     let bundle_actions = self.modules_analyzer.analyze_statement(
  //       &module_id,
  //       &mut self.bundle_variable,
  //       &self.module_graph,
  //       &self.bundle_modules,
  //       &mut self.bundle_external_reference,
  //       &self.context,
  //     )?;

  //     self.actions.extend(bundle_actions);

  //     // step: 4 rename and patch ast
  //     self
  //       .modules_analyzer
  //       .module_analyzer_mut(module_id)
  //       .unwrap()
  //       .patch_ast(&self.context, &self.bundle_variable)?;
  //   }

  //   // step: 5 execute actions
  //   self.modules_analyzer.execute_actions(
  //     &self.module_graph,
  //     &mut self.bundle_variable,
  //     &self.context,
  //   )?;

  //   Ok(())
  // }

  // // step: 3 execute actions for single bundle
  // pub fn execute_bundle_actions(&mut self) -> Result<()> {
  //   if self.bundle_ordered_modules.is_empty() {
  //     return Ok(());
  //   }

  //   let mut patch_to_first_module = vec![];
  //   let mut patch_to_last_module = vec![];

  //   println!("bundle actions: {:#?}", self.actions);
  //   println!("bundle_external_reference: {:#?}", self.bundle_external_reference);

  //   for action in &self.actions {
  //     match action {
  //       BundleAction::SaveImport(m_id) => {
  //         if let Some(import) = self.bundle_external_reference.import(m_id) {
  //           if import.named.is_empty() && import.namespace.is_none() && import.default.is_none() {
  //             continue;
  //           }

  //           let mut specifiers = vec![];

  //           for (imported, local) in import.named.iter() {
  //             specifiers.push(farmfe_core::swc_ecma_ast::ImportSpecifier::Named(
  //               ImportNamedSpecifier {
  //                 span: DUMMY_SP,
  //                 local: self.bundle_variable.render_name(*local).as_str().into(),
  //                 imported: Some(farmfe_core::swc_ecma_ast::ModuleExportName::Ident(
  //                   imported.as_str().into(),
  //                 )),
  //                 is_type_only: false,
  //               },
  //             ));
  //           }

  //           if let Some(namespace) = import.namespace.as_ref() {
  //             specifiers.push(farmfe_core::swc_ecma_ast::ImportSpecifier::Namespace(
  //               ImportStarAsSpecifier {
  //                 span: DUMMY_SP,
  //                 local: self.bundle_variable.render_name(*namespace).as_str().into(),
  //               },
  //             ));
  //           }

  //           if let Some(default) = import.default.as_ref() {
  //             specifiers.push(farmfe_core::swc_ecma_ast::ImportSpecifier::Default(
  //               ImportDefaultSpecifier {
  //                 span: DUMMY_SP,
  //                 local: self.bundle_variable.render_name(*default).as_str().into(),
  //               },
  //             ));
  //           }

  //           patch_to_first_module.push(ModuleItem::ModuleDecl(ModuleDecl::Import(ImportDecl {
  //             span: DUMMY_SP,
  //             specifiers,
  //             src: Box::new(Str {
  //               span: DUMMY_SP,
  //               value: m_id.relative_path().into(),
  //               raw: None,
  //             }),
  //             type_only: false,
  //             with: None,
  //             phase: farmfe_core::swc_ecma_ast::ImportPhase::Evaluation,
  //           })));
  //         }
  //       }
  //       BundleAction::SaveExport(module_id) => {
  //         if let Some(module_analyzer) = self.modules_analyzer.module_analyzer(module_id) {
  //           for export in module_analyzer.exports_stmts() {
  //             for specify in &export.specifiers {
  //               match specify {
  //                 ExportSpecifierInfo::All(_) => {}
  //                 ExportSpecifierInfo::Named { .. } => {
  //                   self.bundle_external_reference.sync_export(specify, &None);
  //                 }
  //                 ExportSpecifierInfo::Default(_) => {
  //                   self.bundle_external_reference.sync_export(specify, &None);
  //                 }
  //                 ExportSpecifierInfo::Namespace(_) => {
  //                   self.bundle_external_reference.sync_export(specify, &None);
  //                 }
  //               }
  //             }
  //           }
  //         };
  //       }
  //     }
  //   }

  //   for (source, export) in self.bundle_external_reference.external_export_map.iter() {
  //     patch_to_last_module.extend(generate_export(Some(source), export, &self.bundle_variable));
  //   }

  //   if let Some(export) = self.bundle_external_reference.export.as_ref() {
  //     patch_to_last_module.extend(generate_export(None, export, &self.bundle_variable));
  //   }

  //   if !patch_to_first_module.is_empty() {
  //     if let Some(module_analyzer) = self
  //       .bundle_ordered_modules
  //       .first()
  //       .map(|item| self.modules_analyzer.module_analyzer_mut(item))
  //       .flatten()
  //     {
  //       let ast = &mut module_analyzer.ast;

  //       ast.body = patch_to_first_module
  //         .into_iter()
  //         .chain(ast.body.take().into_iter())
  //         .collect();
  //     };
  //   }

  //   if !patch_to_last_module.is_empty() {
  //     if let Some(module_analyzer) = self
  //       .bundle_ordered_modules
  //       .last()
  //       .map(|id| self.modules_analyzer.module_analyzer_mut(id))
  //       .flatten()
  //     {
  //       let ast = &mut module_analyzer.ast;

  //       ast.body = ast
  //         .body
  //         .take()
  //         .into_iter()
  //         .chain(patch_to_last_module)
  //         .collect();
  //     };
  //   }

  //   Ok(())
  // }

  // // step: 4 generate bundle code
  pub fn codegen(&mut self, module_analyzer_manager: &mut ModuleAnalyzerManager) -> Result<Bundle> {
    let mut bundle = Bundle::new(BundleOptions {
      separator: Some('\n'),
      intro: None,
      trace_source_map_chain: Some(false),
    });

    for module_id in &self.ordered_modules {
      let module = self
        .module_graph
        .module(&module_id)
        .unwrap_or_else(|| panic!("Module not found: {:?}", module_id));
      let module_analyzer = module_analyzer_manager
        .module_analyzer_mut(module_id)
        .unwrap();

      let comments: SingleThreadedComments = module.meta.as_script().comments.clone().into();

      let sourcemap_enabled = self.context.config.sourcemap.enabled(module.immutable);

      try_with(
        module_analyzer.cm.clone(),
        &self.context.meta.script.globals,
        || {
          module_analyzer
            .ast
            .visit_mut_with(&mut fixer(Some(&comments)));
        },
      )?;

      let mut mappings = vec![];
      let code_bytes = codegen_module(
        &module_analyzer.ast,
        self.context.config.script.target,
        module_analyzer.cm.clone(),
        if sourcemap_enabled {
          Some(&mut mappings)
        } else {
          None
        },
        false,
        Some(CodeGenCommentsConfig {
          comments: &comments,
          config: &self.context.config.comments,
        }),
      )
      .expect("failed generate script mode");

      let code = String::from_utf8(code_bytes).expect("failed to convert code bytes to string");

      let mut source_map_chain = vec![];

      if sourcemap_enabled {
        let sourcemap = build_source_map(module_analyzer.cm.clone(), &mappings);
        let mut buf = vec![];
        sourcemap
          .to_writer(&mut buf)
          .map_err(|e| CompilationError::RenderScriptModuleError {
            id: module_id.to_string(),
            source: Some(Box::new(e)),
          })?;
        let map = Arc::new(String::from_utf8(buf).unwrap());

        source_map_chain = module.source_map_chain.clone();
        source_map_chain.push(map);
      }

      let mut module = MagicString::new(
        &code,
        Some(MagicStringOptions {
          filename: Some(module_id.resolved_path_with_query(&self.context.config.root)),
          source_map_chain,
          ..Default::default()
        }),
      );

      if matches!(self.context.config.mode, Mode::Development) {
        module.prepend(&format!("// module_id: {}\n", module_id.to_string()));
      }

      bundle.add_source(module, None).unwrap();
    }

    Ok(bundle)
  }
}
