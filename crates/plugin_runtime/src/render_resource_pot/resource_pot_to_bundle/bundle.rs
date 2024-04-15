use std::{
  borrow::Cow,
  cell::RefMut,
  collections::{HashMap, HashSet, VecDeque},
  mem::{self, replace},
  sync::Arc,
};

use farmfe_core::{
  context::CompilationContext,
  error::Result,
  module::{module_graph::ModuleGraph, ModuleId},
  swc_common::DUMMY_SP,
  swc_ecma_ast::{
    self, BindingIdent, ClassDecl, Decl, EmptyStmt, Expr, FnDecl, Ident, KeyValueProp, ModuleDecl,
    ModuleItem, ObjectLit, Pat, Prop, PropName, PropOrSpread, Stmt, VarDecl, VarDeclKind,
    VarDeclarator,
  },
};
use farmfe_toolkit::{script::swc_try_with::try_with, swc_ecma_visit::VisitMutWith};

use super::{
  bundle_external::BundleReference,
  defined_idents_collector::RenameIdent,
  modules_analyzer::module_analyzer::{
    ExportInfo, ExportSpecifierInfo, ModuleAnalyzer, StmtAction, Variable,
  },
  uniq_name::{safe_name_form_module_id, BundleVariable, UniqName},
};

pub struct ModuleAnalyzerManager {
  pub module_map: HashMap<ModuleId, ModuleAnalyzer>,
  pub namespace: HashSet<ModuleId>,
  pub namespace_uniq_named: HashMap<ModuleId, (usize, usize)>,
}

impl ModuleAnalyzerManager {
  pub fn new(module_map: HashMap<ModuleId, ModuleAnalyzer>) -> Self {
    Self {
      module_map,
      namespace: HashSet::new(),
      namespace_uniq_named: HashMap::new(),
    }
  }

  pub fn extract_modules_statements(
    &mut self,
    modules: &Vec<&ModuleId>,
    module_graph: &ModuleGraph,
    mut bundle_variable: RefMut<BundleVariable>,
  ) -> Result<()> {
    for module_id in modules {
      if let Some(module_analyzer) = self.module_map.get_mut(module_id) {
        module_analyzer.extract_statement(module_graph, &mut bundle_variable)?;
      }
    }

    Ok(())
  }

  #[inline]
  pub fn module_analyzer(&self, module_id: &ModuleId) -> Option<&ModuleAnalyzer> {
    self.module_map.get(module_id)
  }

  #[inline]
  pub fn module_analyzer_mut(&mut self, module_id: &ModuleId) -> Option<&mut ModuleAnalyzer> {
    self.module_map.get_mut(module_id)
  }

  #[inline]
  pub fn is_in_namespace(&self, module_id: &ModuleId) -> bool {
    self.namespace.contains(module_id)
  }

  pub fn export_names(&self, module_id: &ModuleId) -> Vec<(ExportInfo, ModuleId)> {
    let mut exports: Vec<(ExportInfo, ModuleId)> = vec![];

    let exports_stmts = if let Some(module_analyzer) = self.module_analyzer(module_id) {
      if let Some(export_names) = &module_analyzer.export_names {
        return export_names.clone();
      }

      module_analyzer
        .exports_stmts()
        .into_iter()
        .cloned()
        .collect::<Vec<_>>()
    } else {
      vec![]
    };

    for export in exports_stmts {
      if let Some(source) = export.source.as_ref() {
        let module_analyzer_option = self.module_analyzer(source);

        if module_analyzer_option.is_none() || module_analyzer_option.is_some_and(|m| m.external) {
          exports.push((export, module_id.clone()));
          continue;
        }
      }

      for specify in export.specifiers.iter() {
        match specify {
          ExportSpecifierInfo::All(_) => {
            if let Some(source) = &export.source {
              let result = self.export_names(source);
              exports.extend(result);
            }
          }

          _ => {
            if let Some(source) = &export.source {
              let result = self.export_names(source);
              exports.extend(result);
            } else {
              exports.push((
                ExportInfo {
                  source: export.source.clone(),
                  specifiers: vec![specify.clone()],
                  stmt_id: export.stmt_id,
                },
                module_id.clone(),
              ));
            }
          }
        }
      }
    }

    exports
  }

  pub fn patch_module_analyzer_ast(
    &mut self,
    module_id: &ModuleId,
    context: &Arc<CompilationContext>,
    bundle_variable: &mut BundleVariable,
    external_reference: &mut BundleReference,
  ) -> Result<()> {
    let namespace = self.namespace_uniq_named.get(module_id).cloned();

    self.patch_module(
      module_id,
      context,
      bundle_variable,
      external_reference,
      namespace,
    )?;

    Ok(())
  }

  fn patch_module(
    &mut self,
    module_id: &ModuleId,
    context: &Arc<CompilationContext>,
    bundle_variable: &mut BundleVariable,
    external_reference: &mut BundleReference,
    namespace: Option<(usize, usize)>,
  ) -> Result<()> {
    if let Some(module_analyzer) = self.module_analyzer_mut(module_id) {
      let mut stmt_actions = module_analyzer
        .statement_actions
        .clone()
        .into_iter()
        .collect::<Vec<_>>();
      stmt_actions.sort_by(|a, b| b.index().cmp(&a.index()));

      try_with(
        module_analyzer.cm.clone(),
        &context.meta.script.globals,
        || {
          stmt_actions.iter().for_each(|action| {

          if let Some(index) = action.index() {
            let stmt = replace(
            &mut module_analyzer.ast.body[index],
            ModuleItem::Stmt(Stmt::Empty(EmptyStmt { span: DUMMY_SP })),
            );

            match action {
              StmtAction::StripExport(_) => match stmt {
                ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(export_decl)) => {
                  module_analyzer.ast.body[index] = ModuleItem::Stmt(Stmt::Decl(export_decl.decl))
                }
                _ => {}
              },

              StmtAction::StripDefaultExport(_, rename) => match stmt {
                  ModuleItem::ModuleDecl(ModuleDecl::ExportDefaultDecl(export_decl)) => {
                    let rendered_name = bundle_variable.render_name(*rename);
                    module_analyzer.ast.body[index] = ModuleItem::Stmt(Stmt::Decl(match export_decl.decl {
                      swc_ecma_ast::DefaultDecl::Class(class) => {
                        Decl::Class(
                          ClassDecl {
                            ident: Ident::from(rendered_name.as_str()),
                            declare: false,
                            class: class.class,
                          },
                        )
                      },
                      swc_ecma_ast::DefaultDecl::Fn(f) => {
                        Decl::Fn(FnDecl {
                          ident: Ident::from(rendered_name.as_str()),
                          declare: false,
                          function: f.function,
                        })
                      },
                      _ => {
                        unreachable!(
                          "export_default_decl.decl should not be anything clone() other than a class, function"
                        )
                      },
                    }));
                  }
                  _ => {
                  }
                }

              StmtAction::DeclDefaultExpr(_, var) => {
                if let ModuleItem::ModuleDecl(ModuleDecl::ExportDefaultExpr(export_default_decl)) = stmt
                {
                  // TODO: 看看 case
                  module_analyzer.ast.body[index] =
                    ModuleItem::Stmt(Stmt::Decl(Decl::Var(Box::new(VarDecl {
                      span: DUMMY_SP,
                      kind: swc_ecma_ast::VarDeclKind::Var,
                      declare: false,
                      decls: vec![VarDeclarator {
                        span: DUMMY_SP,
                        name: swc_ecma_ast::Pat::Ident(BindingIdent {
                          id: Ident::from(bundle_variable.render_name(*var).as_str()),
                          type_ann: None,
                        }),
                        init: Some(export_default_decl.expr),
                        definite: false,
                      }],
                    }))));
                }
              }
              StmtAction::StripImport(_) | StmtAction::RemoveImport(_) | StmtAction::RemoveExport(_) => {}
              _ => {}
            }
          }

        });
        },
      )?;
    };

    let mut namespace_asts = vec![];

    // TODO: 查询 importer 是否与包含 other bundle or 入口文件
    if let Some((local, named_as)) = namespace {
      let namespace = bundle_variable.name(local);

      let mut statements = self
        .module_analyzer(module_id)
        .map(|item| {
          item
            .exports_stmts()
            .into_iter()
            .map(|item| Cow::Borrowed(item))
            .collect::<VecDeque<_>>()
        })
        .unwrap();

      let mut props: Vec<PropOrSpread> = vec![];

      while let Some(export) = statements.pop_front() {
        for specify in &export.specifiers {
          match specify {
            ExportSpecifierInfo::All(_) => {
              if let Some(source) = &export.source {
                let export_names = self.export_names(source);

                // TODO: 看下 case
                for (export, _) in export_names {
                  statements.push_back(Cow::Owned(export));
                }
              }
            }
            ExportSpecifierInfo::Named(named) => {
              if let Some(exported) = &named.1 {
                let exported = bundle_variable.name(*exported);
                let local_ident = bundle_variable.render_name(named.local());

                props.push(PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
                  key: PropName::Str(exported.as_str().into()),
                  value: Box::new(Expr::Ident(Ident::from(local_ident.as_str()))),
                }))));
              } else {
                let local = bundle_variable.var_by_index(named.local());
                let local_key = local.origin_name();
                let local_ident = local.render_name();

                props.push(PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
                  key: PropName::Str(local_key.as_str().into()),
                  value: Box::new(Expr::Ident(Ident::from(local_ident.as_str()))),
                }))));
              };
            }
            ExportSpecifierInfo::Default(_) => {
              // let default_ident = bundle_variable.render_name(*default);

              // props.push(PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
              //   key: PropName::Ident(Ident::from("default")),
              //   value: Box::new(Expr::Ident(Ident::from(default_ident.as_str()))),
              // }))));
            }
            ExportSpecifierInfo::Namespace(ns) => {
              let namespace = bundle_variable.var_by_index(*ns);

              let ns_key = namespace.origin_name();
              let ns_value = namespace.render_name();

              props.push(PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
                key: PropName::Str(ns_key.as_str().into()),
                value: Box::new(Expr::Ident(ns_value.as_str().into())),
              }))));
            }
          }
        }
      }

      namespace_asts.push(ModuleItem::Stmt(Stmt::Decl(Decl::Var(Box::new(VarDecl {
        span: DUMMY_SP,
        kind: VarDeclKind::Var,
        declare: false,
        decls: vec![VarDeclarator {
          span: DUMMY_SP,
          name: Pat::Ident(BindingIdent {
            id: Ident::new(namespace.as_str().into(), DUMMY_SP),
            type_ann: None,
          }),
          init: Some(Box::new(Expr::Object(ObjectLit {
            span: DUMMY_SP,
            props: props,
          }))),
          definite: false,
        }],
      })))));

      // TODO: 压缩 namespace 导出名称
      // external_reference.sync_export(&ExportSpecifierInfo::Named(raw_namespace.into()), &None)
    }

    if let Some(module_analyzer) = self.module_analyzer_mut(module_id) {
      module_analyzer.ast.body.extend(namespace_asts);

      let rename_map = module_analyzer.build_rename_map(bundle_variable);

      module_analyzer.ast.body = mem::take(&mut module_analyzer.ast.body)
        .into_iter()
        .filter_map(|item| match item {
          ModuleItem::Stmt(Stmt::Empty(_)) => None,
          _ => Some(item),
        })
        .collect::<Vec<_>>();

      module_analyzer
        .ast
        .visit_mut_with(&mut RenameIdent::new(rename_map));
    }

    Ok(())
  }

  pub fn link(&mut self, bundle_variable: &mut BundleVariable, context: &Arc<CompilationContext>) {
    let mut uniq_name = UniqName::new();

    for module_analyzer in self.module_map.values_mut() {
      for (namespace_module_id, as_name) in module_analyzer.namespace_importers() {
        self.namespace.insert(namespace_module_id.clone());

        let module_safe_name = safe_name_form_module_id(&namespace_module_id, context);
        let uniq_name_safe_name = uniq_name.uniq_name(&module_safe_name);

        uniq_name.insert(&uniq_name_safe_name);

        let var = bundle_variable.register_var(
          &namespace_module_id,
          &uniq_name_safe_name.as_str().into(),
          false,
        );

        self
          .namespace_uniq_named
          .insert(namespace_module_id, (var, as_name));
      }
    }

    // {
    //   "namespace1/moduleA.js": "moduleA",
    //   "namespace2/moduleA.js": "moduleA$1"
    // }
    // for namespace_module_id in &self.namespace {
    //   let module_safe_name = safe_name_form_module_id(namespace_module_id, context);
    //   let uniq_name_safe_name = uniq_name.uniq_name(&module_safe_name);

    //   uniq_name.insert(&uniq_name_safe_name);

    //   self.namespace_uniq_named.insert(
    //     namespace_module_id.clone(),
    //     bundle_variable.register_var(
    //       namespace_module_id,
    //       &uniq_name_safe_name.as_str().into(),
    //       false,
    //     ),
    //   );
    // }
  }
}
