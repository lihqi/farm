use std::{
  collections::{HashMap, HashSet},
  sync::Arc,
};

use farmfe_core::{
  context::CompilationContext,
  error::{CompilationError, Result},
  module::{module_graph::ModuleGraph, ModuleId},
  swc_common::DUMMY_SP,
  swc_ecma_ast::{
    BindingIdent, Decl, Expr, Ident, KeyValueProp, ModuleItem, ObjectLit, Pat, Prop, PropName,
    PropOrSpread, SpreadElement, Stmt, VarDecl, VarDeclKind, VarDeclarator,
  },
};
use farmfe_toolkit::script::swc_try_with::try_with;

use crate::resource_pot_to_bundle::uniq_name::ZipExportAllChain;

use self::module_analyzer::{ExportSpecifierInfo, ImportSpecifierInfo, ModuleAnalyzer, StmtAction};

use super::{
  bundle_analyzer::BundleAction, bundle_external::BundleReference, uniq_name::BundleVariable,
};

pub mod module_analyzer;

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum ModuleAction {
  DeclModuleAllExport(ModuleId),
}

pub struct ModulesAnalyzer {
  module_analyzers: HashMap<ModuleId, ModuleAnalyzer>,
  module_actions: HashSet<ModuleAction>,
}

impl ModulesAnalyzer {
  pub fn new() -> Self {
    Self {
      module_analyzers: HashMap::new(),
      module_actions: HashSet::new(),
    }
  }

  pub fn module_analyzer(&self, module_id: &ModuleId) -> Option<&ModuleAnalyzer> {
    self.module_analyzers.get(module_id)
  }
  pub fn module_analyzer_mut(&mut self, module_id: &ModuleId) -> Option<&mut ModuleAnalyzer> {
    self.module_analyzers.get_mut(module_id)
  }

  // pub fn extract_module_statement(
  //   &mut self,
  //   module_id: &ModuleId,
  //   module_graph: &ModuleGraph,
  //   bundle_variable: &mut BundleVariable,
  //   context: &Arc<CompilationContext>,
  // ) -> Result<()> {
  //   let module = module_graph.module(module_id).unwrap();
  //   let mut analyzer = ModuleAnalyzer::new(&module, context)?;

  //   analyzer.extract_statement(module_graph, bundle_variable)?;

  //   self.module_analyzers.insert((*module_id).clone(), analyzer);

  //   Ok(())
  // }

  // pub fn analyze_statement(
  //   &mut self,
  //   module_id: &ModuleId,
  //   bundle_variable: &mut BundleVariable,
  //   module_graph: &ModuleGraph,
  //   modules_set: &HashSet<&ModuleId>,
  //   bundle_external_reference: &mut BundleReference,
  //   context: &Arc<CompilationContext>,
  // ) -> Result<HashSet<BundleAction>> {
  //   println!("\n\nmodule_id: {}", module_id.to_string());
  //   let module_analyzer = self.module_analyzer(module_id).unwrap();
  //   let is_entry = context
  //     .config
  //     .input
  //     .values()
  //     .any(|item| item == &module_id.resolved_path(&context.config.root));

  //   let module = module_graph.module(module_id).unwrap();
  //   fn check_is_have_other_bundle_issuers(
  //     module_graph: &ModuleGraph,
  //     module_id: &ModuleId,
  //     modules_set: &HashSet<&ModuleId>,
  //   ) -> bool {
  //     let issuers = module_graph.dependents_ids(module_id);
  //     issuers.iter().any(|item| !modules_set.contains(&item))
  //   }
  //   let is_have_other_bundle_issuers = {
  //     let is_runtime = matches!(module.module_type, farmfe_core::module::ModuleType::Runtime);

  //     !is_runtime
  //       && (is_entry || check_is_have_other_bundle_issuers(module_graph, module_id, modules_set))
  //   };

  //   let mut bundle_actions = HashSet::new();
  //   let mut statement_actions = HashSet::new();
  //   let mut modules_actions = HashSet::new();

  //   for statement in &module_analyzer.statements {
  //     if let Some(import) = &statement.import {
  //       let dep_module_id = &import.source;
  //       let module = module_graph.module(&dep_module_id).map(Ok).unwrap_or(Err(
  //         CompilationError::GenericError(format!("not found module {:?}", dep_module_id)),
  //       ))?;

  //       let is_external = module.external;
  //       if !is_external && modules_set.contains(&dep_module_id) {
  //         if import.specifiers.is_empty() {
  //           statement_actions.insert(StmtAction::RemoveImport(statement.id));
  //         } else {
  //           for specify in &import.specifiers {
  //             statement_actions.insert(StmtAction::StripImport(import.stmt_id));
  //             match specify {
  //               ImportSpecifierInfo::Namespace(ns) => {
  //                 bundle_variable.fetch_module_safe_name_and_set_var_rename(
  //                   *ns,
  //                   &dep_module_id,
  //                   context,
  //                 );

  //                 modules_actions.insert(ModuleAction::DeclModuleAllExport(dep_module_id.clone()));
  //               }
  //               ImportSpecifierInfo::Named { local, imported } => {
  //                 // let is_default =
  //                 //   imported.is_some_and(|imported| bundle_variable.name(imported) == "default");
  //                 // if let Some((index, _)) = bundle_variable.find_ident_by_index(
  //                 //   if let Some(imported) = imported {
  //                 //     *imported
  //                 //   } else {
  //                 //     *local
  //                 //   },
  //                 //   &module_id,
  //                 //   &import.source,
  //                 //   module_graph,
  //                 //   &self.module_analyzers,
  //                 //   is_default,
  //                 // ) {
  //                 //   bundle_variable.set_rename(*local, bundle_variable.render_name(index));
  //                 // } else {
  //                 //   bundle_variable.var_mut_by_index(*local).removed = true;
  //                 // };
  //               }
  //               ImportSpecifierInfo::Default(import_ident) => {
  //                 // if let Some((target, _)) = bundle_variable.find_ident_by_index(
  //                 //   *import_ident,
  //                 //   &module_id,
  //                 //   &import.source,
  //                 //   module_graph,
  //                 //   &self.module_analyzers,
  //                 //   true,
  //                 // ) {
  //                 //   bundle_variable.set_rename(*import_ident, bundle_variable.render_name(target));
  //                 // };
  //               }
  //             }
  //           }
  //         }
  //       } else {
  //         for specify in &import.specifiers {
  //           let ensure_exists_uniq_name =
  //             bundle_external_reference.sync_import(&dep_module_id, specify, &bundle_variable)?;
  //           bundle_variable.set_var_uniq_rename(ensure_exists_uniq_name);
  //           let exists_uniq_render_name = bundle_variable.render_name(ensure_exists_uniq_name);

  //           match specify {
  //             ImportSpecifierInfo::Namespace(ns) => {
  //               bundle_variable.set_rename(*ns, exists_uniq_render_name);
  //             }
  //             ImportSpecifierInfo::Named { local, imported: _ } => {
  //               bundle_variable.set_rename(*local, exists_uniq_render_name);
  //             }
  //             ImportSpecifierInfo::Default(default) => {
  //               bundle_variable.set_rename(*default, exists_uniq_render_name);
  //             }
  //           }
  //         }
  //         bundle_actions.insert(BundleAction::SaveImport(dep_module_id.clone()));
  //         statement_actions.insert(StmtAction::RemoveImport(statement.id));
  //       }
  //     }

  //     for decl in statement.defined.iter() {
  //       bundle_variable.set_var_uniq_rename(*decl);
  //     }

  //     if let Some(export) = &statement.export {
  //       // if is_have_other_bundle_issuers {
  //       //   bundle_actions.insert(BundleAction::SaveExport(module_id.clone()));
  //       // }
  //       if export.specifiers.is_empty() {
  //         statement_actions.insert(StmtAction::RemoveExport(statement.id));
  //       } else {
  //         for specify in &export.specifiers {
  //           // TODO: other bundle export
  //           match specify {
  //             ExportSpecifierInfo::All(_) => {
  //               statement_actions.insert(StmtAction::RemoveExport(statement.id));

  //               if let Some(source) = export.source.as_ref() {
  //                 if let Some(ZipExportAllChain {
  //                   chains,
  //                   is_use_declare_replace_export,
  //                 }) = bundle_variable.try_find_export_all(
  //                   module_id,
  //                   source,
  //                   module_graph,
  //                   &self.module_analyzers,
  //                 ) {
  //                   let source_module_id = chains.last().unwrap();
  //                   let source_module = module_graph.module(&source_module_id).unwrap();

  //                   let is_in_self_bundle = modules_set.contains(&source_module_id);

  //                   // export * from "external_modules";
  //                   if source_module.external || !is_in_self_bundle {
  //                     bundle_external_reference
  //                       .sync_export(specify, &Some(source_module_id.clone()));
  //                   }
  //                   // export * from "self_bundle_modules";
  //                   else if is_in_self_bundle {
  //                     modules_actions
  //                       .insert(ModuleAction::DeclModuleAllExport(source_module_id.clone()));

  //                     bundle_actions.insert(BundleAction::SaveExport(source_module_id.clone()));
  //                   }
  //                   // other bundle
  //                   // export * from "other_bundle_module"
  //                   else {
  //                     // TODO: other bundle export
  //                   }
  //                 };
  //               } else {
  //                 unreachable!("export * should have source");
  //               }
  //             }

  //             ExportSpecifierInfo::Named(named) => {
  //               let index = named.export_as();

  //               // if let Some(source) = export.source.as_ref() {
  //               //   // export { name } from './external'
  //               //   if let Some((result, origin)) = bundle_variable.find_ident_by_index(
  //               //     index,
  //               //     source,
  //               //     module_graph,
  //               //     &self.module_analyzers,
  //               //     bundle_variable.name(index) == "default",
  //               //   ) {
  //               //     if let Some(module) = module_graph.module(&origin) {
  //               //       let is_self_bundle = modules_set.contains(&origin);
  //               //       // export { name } from './other_bundle_module'
  //               //       // println!("export named: {} {} {:#?}", module.external, is_self_bundle, bundle_variable.render_name(index));
  //               //       if module.external && !is_self_bundle {
  //               //         bundle_external_reference.sync_export(specify, &Some(origin.clone()));
  //               //       }
  //               //       // export { name } from './self_bundle_module'
  //               //       else if is_self_bundle {
  //               //         // do nothing
  //               //         bundle_external_reference.sync_export(specify, &None);
  //               //       }
  //               //       // export { name } from './other_bundle_module'
  //               //       else {
  //               //         todo!("export from other bundle export")
  //               //       }
  //               //     } else {
  //               //       unreachable!("not found module")
  //               //     }
  //               //     bundle_variable.set_rename(*local, bundle_variable.render_name(result));
  //               //   };
  //               // } else {
  //               //   // export { name }
  //               //   bundle_variable.set_var_uniq_rename(*local);
  //               // }

  //               statement_actions.insert(StmtAction::StripExport(statement.id));
  //             }

  //             ExportSpecifierInfo::Default(index) => {
  //               let var_ident = bundle_variable.name(*index);
  //               // export default expr, eg: 1 + 1
  //               if var_ident == "default" {
  //                 let safe_name =
  //                   bundle_variable.fetch_module_safe_name_and_set(module_id, context);

  //                 bundle_variable
  //                   .set_var_uniq_rename_string(*index, format!("{}_default", safe_name));

  //                 statement_actions
  //                   .insert(StmtAction::DeclDefaultExpr(statement.id, index.clone()));
  //               } else {
  //                 bundle_variable.set_var_uniq_rename(*index);
  //                 statement_actions
  //                   .insert(StmtAction::StripDefaultExport(statement.id, index.clone()));
  //               }
  //             }

  //             ExportSpecifierInfo::Namespace(ns) => {
  //               if let Some(ref dep_module_id) = export.source {
  //                 let dep_module = module_graph.module(&dep_module_id).unwrap();
  //                 let is_self_bundle = modules_set.contains(&dep_module_id);
  //                 // export * as ns from './external_module'
  //                 if dep_module.external && !is_self_bundle {
  //                   bundle_external_reference.sync_export(specify, &Some(dep_module_id.clone()));
  //                 }
  //                 // export * as ns from './self_bundle_module'
  //                 else if is_self_bundle {
  //                   bundle_variable.fetch_module_safe_name_and_set_var_rename(
  //                     *ns,
  //                     &dep_module_id,
  //                     context,
  //                   );

  //                   modules_actions
  //                     .insert(ModuleAction::DeclModuleAllExport(dep_module_id.clone()));
  //                 }
  //                 // export * as ns from './other_bundle_module'
  //                 else {
  //                   bundle_variable.fetch_module_safe_name_and_set_var_rename(
  //                     *ns,
  //                     &dep_module_id,
  //                     context,
  //                   );

  //                   // modules_actions
  //                   //   .insert(ModuleAction::DeclModuleAllExport(dep_module_id.clone()));
  //                 }

  //                 statement_actions.insert(StmtAction::RemoveExport(statement.id));
  //               } else {
  //                 unreachable!("Namespace export should have source")
  //               }
  //             }
  //           }
  //         }
  //       }
  //     }
  //   }

  //   self
  //     .module_analyzer_mut(module_id)
  //     .unwrap()
  //     .statement_actions
  //     .extend(statement_actions);

  //   self.module_actions.extend(modules_actions);

  //   Ok(bundle_actions)
  // }

  // pub fn execute_actions(
  //   &mut self,
  //   module_graph: &ModuleGraph,
  //   bundle_variable: &mut BundleVariable,
  //   context: &Arc<CompilationContext>,
  // ) -> Result<()> {
  //   for action in &self.module_actions {
  //     match action {
  //       // namespace ignore default
  //       ModuleAction::DeclModuleAllExport(module_id) => {
  //         let module_analyzer = self
  //           .module_analyzers
  //           .get_mut(module_id)
  //           .map(Ok)
  //           .unwrap_or_else(|| {
  //             Err(CompilationError::GenericError(format!(
  //               "failed get module {} from this bundle",
  //               module_id.to_string()
  //             )))
  //           })?;
  //         let statements = module_analyzer.exports_stmts();

  //         let mut props: Vec<PropOrSpread> = vec![];

  //         for export in statements {
  //           for specify in &export.specifiers {
  //             match specify {
  //               ExportSpecifierInfo::All(_) => {
  //                 if let Some(dep_module_id) = &export.source {
  //                   let export_ident =
  //                     bundle_variable.fetch_module_safe_name_and_set(&dep_module_id, context);

  //                   // TODO: wrap .assign
  //                   props.push(PropOrSpread::Spread(SpreadElement {
  //                     dot3_token: DUMMY_SP,
  //                     expr: Box::new(Expr::Ident(Ident::from(export_ident.as_str()))),
  //                   }));
  //                 }
  //               }
  //               ExportSpecifierInfo::Named(named) => {
  //                 if let Some(exported) = &named.1 {
  //                   let exported = bundle_variable.name(*exported);
  //                   let local_ident = bundle_variable.render_name(named.local());

  //                   props.push(PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
  //                     key: PropName::Ident(Ident::from(exported.as_str())),
  //                     value: Box::new(Expr::Ident(Ident::from(local_ident.as_str()))),
  //                   }))));
  //                 } else {
  //                   let local = bundle_variable.var_by_index(named.local());
  //                   let local_key = local.var.0.to_string();
  //                   let local_ident = local.rename.clone().unwrap_or(local_key.clone());

  //                   props.push(PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
  //                     key: PropName::Ident(Ident::from(local_key.as_str())),
  //                     value: Box::new(Expr::Ident(Ident::from(local_ident.as_str()))),
  //                   }))));
  //                 };
  //               }
  //               ExportSpecifierInfo::Default(default) => {
  //                 // let default_ident = bundle_variable.render_name(*default);

  //                 // props.push(PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
  //                 //   key: PropName::Ident(Ident::from("default")),
  //                 //   value: Box::new(Expr::Ident(Ident::from(default_ident.as_str()))),
  //                 // }))));
  //               }
  //               ExportSpecifierInfo::Namespace(ns) => {
  //                 let namespace = bundle_variable.var_by_index(*ns);
  //                 let ns_key = namespace.origin_name();
  //                 let ns_value = namespace.render_name();

  //                 props.push(PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
  //                   key: PropName::Ident(Ident::from(ns_key.as_str())),
  //                   value: Box::new(Expr::Ident(Ident::from(ns_value.as_str()))),
  //                 }))));
  //               }
  //             }
  //           }
  //         }

  //         if let Some(module_analyzer) = self.module_analyzers.get_mut(&module_id) {
  //           let export_ident = bundle_variable.fetch_module_safe_name_and_set(module_id, context);

  //           try_with(
  //             module_analyzer.cm.clone(),
  //             &context.meta.script.globals,
  //             || {
  //               module_analyzer
  //                 .ast
  //                 .body
  //                 .push(ModuleItem::Stmt(Stmt::Decl(Decl::Var(Box::new(VarDecl {
  //                   span: DUMMY_SP,
  //                   kind: VarDeclKind::Var,
  //                   declare: false,
  //                   decls: vec![VarDeclarator {
  //                     span: DUMMY_SP,
  //                     name: Pat::Ident(BindingIdent {
  //                       id: Ident::new(export_ident.as_str().into(), DUMMY_SP),
  //                       type_ann: None,
  //                     }),
  //                     init: Some(Box::new(Expr::Object(ObjectLit {
  //                       span: DUMMY_SP,
  //                       props: props,
  //                     }))),
  //                     definite: false,
  //                   }],
  //                 })))));
  //             },
  //           )?;
  //         };
  //       }
  //     }
  //   }

  //   Ok(())
  // }
}
