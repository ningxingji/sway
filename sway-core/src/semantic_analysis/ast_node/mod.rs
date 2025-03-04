pub mod code_block;
pub mod declaration;
pub mod expression;
pub mod modes;

pub(crate) use expression::*;
pub(crate) use modes::*;

use crate::{
    language::{parsed::*, ty},
    semantic_analysis::*,
    type_system::*,
    types::DeterministicallyAborts,
    Ident,
};

use sway_error::{
    handler::{ErrorEmitted, Handler},
    warning::{CompileWarning, Warning},
};
use sway_types::{span::Span, Spanned};

impl ty::TyAstNode {
    pub(crate) fn type_check(
        handler: &Handler,
        ctx: TypeCheckContext,
        node: AstNode,
    ) -> Result<Self, ErrorEmitted> {
        let type_engine = ctx.engines.te();
        let decl_engine = ctx.engines.de();
        let engines = ctx.engines();

        let node = ty::TyAstNode {
            content: match node.content.clone() {
                AstNodeContent::UseStatement(a) => {
                    let path = if a.is_absolute {
                        a.call_path.clone()
                    } else {
                        ctx.namespace.find_module_path(&a.call_path)
                    };
                    let _ = match a.import_type {
                        ImportType::Star => {
                            // try a standard starimport first
                            let star_import_handler = Handler::default();
                            let import = ctx.namespace.star_import(
                                &star_import_handler,
                                &path,
                                engines,
                                a.is_absolute,
                            );
                            if import.is_ok() {
                                handler.append(star_import_handler);
                                import
                            } else {
                                // if it doesn't work it could be an enum star import
                                if let Some((enum_name, path)) = path.split_last() {
                                    let variant_import_handler = Handler::default();
                                    let variant_import = ctx.namespace.variant_star_import(
                                        &variant_import_handler,
                                        path,
                                        engines,
                                        enum_name,
                                        a.is_absolute,
                                    );
                                    if variant_import.is_ok() {
                                        handler.append(variant_import_handler);
                                        variant_import
                                    } else {
                                        handler.append(star_import_handler);
                                        import
                                    }
                                } else {
                                    handler.append(star_import_handler);
                                    import
                                }
                            }
                        }
                        ImportType::SelfImport(_) => ctx.namespace.self_import(
                            handler,
                            engines,
                            &path,
                            a.alias.clone(),
                            a.is_absolute,
                        ),
                        ImportType::Item(ref s) => {
                            // try a standard item import first
                            let item_import_handler = Handler::default();
                            let import = ctx.namespace.item_import(
                                &item_import_handler,
                                engines,
                                &path,
                                s,
                                a.alias.clone(),
                                a.is_absolute,
                            );

                            if import.is_ok() {
                                handler.append(item_import_handler);
                                import
                            } else {
                                // if it doesn't work it could be an enum variant import
                                if let Some((enum_name, path)) = path.split_last() {
                                    let variant_import_handler = Handler::default();
                                    let variant_import = ctx.namespace.variant_import(
                                        &variant_import_handler,
                                        engines,
                                        path,
                                        enum_name,
                                        s,
                                        a.alias.clone(),
                                        a.is_absolute,
                                    );
                                    if variant_import.is_ok() {
                                        handler.append(variant_import_handler);
                                        variant_import
                                    } else {
                                        handler.append(item_import_handler);
                                        import
                                    }
                                } else {
                                    handler.append(item_import_handler);
                                    import
                                }
                            }
                        }
                    };
                    ty::TyAstNodeContent::SideEffect(ty::TySideEffect {
                        side_effect: ty::TySideEffectVariant::UseStatement(ty::TyUseStatement {
                            alias: a.alias,
                            call_path: a.call_path,
                            is_absolute: a.is_absolute,
                            import_type: a.import_type,
                        }),
                    })
                }
                AstNodeContent::IncludeStatement(_) => {
                    ty::TyAstNodeContent::SideEffect(ty::TySideEffect {
                        side_effect: ty::TySideEffectVariant::IncludeStatement,
                    })
                }
                AstNodeContent::Declaration(decl) => {
                    ty::TyAstNodeContent::Declaration(ty::TyDecl::type_check(handler, ctx, decl)?)
                }
                AstNodeContent::Expression(expr) => {
                    let ctx = ctx
                        .with_type_annotation(type_engine.insert(engines, TypeInfo::Unknown))
                        .with_help_text("");
                    let inner = ty::TyExpression::type_check(handler, ctx, expr.clone())
                        .unwrap_or_else(|err| ty::TyExpression::error(err, expr.span(), engines));
                    ty::TyAstNodeContent::Expression(inner)
                }
                AstNodeContent::ImplicitReturnExpression(expr) => {
                    let ctx =
                        ctx.with_help_text("Implicit return must match up with block's type.");
                    let typed_expr = ty::TyExpression::type_check(handler, ctx, expr.clone())
                        .unwrap_or_else(|err| ty::TyExpression::error(err, expr.span(), engines));
                    ty::TyAstNodeContent::ImplicitReturnExpression(typed_expr)
                }
                AstNodeContent::Error(spans, err) => ty::TyAstNodeContent::Error(spans, err),
            },
            span: node.span,
        };

        if let ty::TyAstNode {
            content: ty::TyAstNodeContent::Expression(ty::TyExpression { .. }),
            ..
        } = node
        {
            if !node
                .type_info(type_engine)
                .can_safely_ignore(type_engine, decl_engine)
            {
                handler.emit_warn(CompileWarning {
                    warning_content: Warning::UnusedReturnValue {
                        r#type: engines.help_out(node.type_info(type_engine)).to_string(),
                    },
                    span: node.span.clone(),
                })
            };
        }

        Ok(node)
    }
}
